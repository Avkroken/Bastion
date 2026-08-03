import Crypto
import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// Hämtar och cachar en enskild extern binär (t.ex. `wireguard-go`,
/// `tailscale`) från en URL, verifierad mot en känd SHA256-checksumma innan
/// den någonsin skrivs till disk som "giltig". Motiveras av VISION.md
/// "Native WireGuard/Tailscale — inget externt beroende": Bastion ska kunna
/// ladda ner+köra dessa verktyg själv istället för att kräva att användaren
/// installerat dem separat, men en nedladdad binär är samma tillitsnivå som
/// ett `curl | sudo bash`-skript om checksumman inte verifieras — se
/// ROADMAP.md för den fulla designmotiveringen.
///
/// Medvetet GENERISK (URL + förväntad checksumma in, verifierad sökväg ut) —
/// varken WireGuard- eller Tailscale-specifik. Anroparen (framtida UI-lager)
/// äger plattforms-/arkitekturval av URL och det publicerade checksum-värdet;
/// den här typen äger bara hämta+verifiera+cacha-mekaniken, så den kan
/// återanvändas för båda verktygen (och andra framtida) utan duplicering.
public enum ExternalBinaryError: Error, Sendable, Equatable {
    case downloadFailed(String)
    case checksumMismatch(expected: String, actual: String)
    case cacheWriteFailed(String)
}

public enum ExternalBinaryFetcher {
    /// Hämtar `url` till `cacheDir/binaryName` om den inte redan finns där
    /// med RÄTT checksumma (idempotent — ett andra anrop med samma
    /// parametrar gör ingen nätverkstrafik alls). En redan cachad fil med FEL
    /// checksumma (korrupt/manipulerad) tas bort och laddas ner på nytt,
    /// aldrig litad på tyst.
    ///
    /// Checksumman verifieras mot de NEDLADDADE bytesen INNAN något skrivs
    /// till disk — en manipulerad/fel binär hamnar aldrig i cachen ens
    /// tillfälligt.
    public static func fetch(
        url: URL,
        expectedSHA256: String,
        cacheDir: URL,
        binaryName: String,
        session: URLSession = .shared
    ) async throws -> URL {
        let destination = cacheDir.appendingPathComponent(binaryName)
        let expected = expectedSHA256.lowercased()

        if let existing = try? Data(contentsOf: destination), sha256Hex(existing) == expected {
            return destination
        }
        // Finns men med fel checksumma (korrupt eller ett gammalt, felaktigt
        // cachat försök) — städa bort tyst, hämta rent på nytt nedan.
        try? FileManager.default.removeItem(at: destination)

        let data: Data
        do {
            let (downloaded, response) = try await session.data(from: url)
            guard let http = response as? HTTPURLResponse, (200...299).contains(http.statusCode) else {
                let status = (response as? HTTPURLResponse)?.statusCode ?? -1
                throw ExternalBinaryError.downloadFailed("HTTP \(status) för \(url.absoluteString)")
            }
            data = downloaded
        } catch let error as ExternalBinaryError {
            throw error
        } catch {
            throw ExternalBinaryError.downloadFailed("\(error)")
        }

        let actual = sha256Hex(data)
        guard actual == expected else {
            throw ExternalBinaryError.checksumMismatch(expected: expected, actual: actual)
        }

        do {
            try FileManager.default.createDirectory(at: cacheDir, withIntermediateDirectories: true)
            // Skriv till en temporär fil i SAMMA katalog, byt sedan namn —
            // en process som läser `destination` mitt under en nedladdning
            // ska aldrig kunna se en halvskriven fil.
            let tmp = cacheDir.appendingPathComponent(".\(binaryName).\(UUID().uuidString).tmp")
            try data.write(to: tmp)
            try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: tmp.path)
            _ = try? FileManager.default.removeItem(at: destination)
            try FileManager.default.moveItem(at: tmp, to: destination)
        } catch {
            throw ExternalBinaryError.cacheWriteFailed("\(error)")
        }

        return destination
    }

    static func sha256Hex(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }
}
