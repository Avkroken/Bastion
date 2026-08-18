import Foundation

// Kubernetes via `kubectl` över SSH. Port av LinuxApp/integrations/src/kubernetes.rs
// — samma uppdelning: rena kommandobyggare och parsning här, SSH-lagret tunt
// ovanpå.
//
// `--no-headers` och inte `-o json`: några hundra poddar som JSON är megabyte
// över en SSH-kanal som redan har ett tak, och `--no-headers` ger samma fält
// kubectl självt visar.

public enum KubernetesError: Error, Sendable, Equatable {
    case invalidName(String)
}

/// Vilken namnrymd ett kommando gäller.
///
/// Egen typ i stället för `String?`: `nil` skulle kunna läsas som antingen
/// "default" eller "alla", och de två är motsatser. Att tvinga fram valet gör
/// felet omöjligt.
public enum KubernetesNamespace: Sendable, Equatable {
    case all
    case named(String)
}

public struct KubernetesPod: Sendable, Equatable {
    public let namespace: String
    public let name: String
    /// `2/3` — klara containrar av totalt. Två tal med en betydelse ihop.
    public let ready: String
    public let status: String
    public let restarts: String

    public init(namespace: String, name: String, ready: String, status: String, restarts: String) {
        self.namespace = namespace
        self.name = name
        self.ready = ready
        self.status = status
        self.restarts = restarts
    }

    /// `Running` räcker INTE. En podd kan stå i Running med 1/3 klara
    /// containrar i timmar — precis det läget man letar efter när något är
    /// fel, och att måla den grön hade dolt felet.
    public var isHealthy: Bool {
        switch status {
        case "Completed", "Succeeded": return true
        case "Running": return readyMatches
        default: return false
        }
    }

    private var readyMatches: Bool {
        let parts = ready.split(separator: "/", maxSplits: 1).map(String.init)
        guard parts.count == 2, !parts[0].isEmpty else { return false }
        return parts[0] == parts[1]
    }
}

public struct KubernetesDeployment: Sendable, Equatable {
    public let namespace: String
    public let name: String
    public let ready: String

    public init(namespace: String, name: String, ready: String) {
        self.namespace = namespace
        self.name = name
        self.ready = ready
    }

    public var isFullyAvailable: Bool {
        let parts = ready.split(separator: "/", maxSplits: 1).map(String.init)
        guard parts.count == 2, !parts[0].isEmpty else { return false }
        return parts[0] == parts[1]
    }
}

public struct KubernetesNode: Sendable, Equatable {
    public let name: String
    /// `Ready`, `NotReady` eller `Ready,SchedulingDisabled`.
    public let status: String
    public let version: String

    public init(name: String, status: String, version: String) {
        self.name = name
        self.status = status
        self.version = version
    }

    /// Delsträngsmatchning, inte likhet: en avstängd men frisk nod
    /// rapporteras som `Ready,SchedulingDisabled`.
    public var isReady: Bool {
        status.split(separator: ",").contains("Ready")
    }

    public var isCordoned: Bool {
        status.split(separator: ",").contains("SchedulingDisabled")
    }
}

public enum KubernetesService {
    /// RFC 1123-etiketter: gemener, siffror och bindestreck, måste börja och
    /// sluta alfanumeriskt, max 63 tecken.
    ///
    /// Avsiktligt SNÄVARE än Dockers — inte överdriven försiktighet utan vad
    /// API-servern accepterar. Att den är snäv gör den också till ett fullgott
    /// injektionsskydd.
    static let namePattern = try! NSRegularExpression(
        pattern: "^[a-z0-9]([a-z0-9-]*[a-z0-9])?$"
    )

    public static func validate(_ name: String) throws -> String {
        let range = NSRange(name.startIndex..<name.endIndex, in: name)
        guard name.count <= 63, namePattern.firstMatch(in: name, range: range) != nil else {
            throw KubernetesError.invalidName(name)
        }
        return name
    }

    static func flag(_ namespace: KubernetesNamespace) throws -> String {
        switch namespace {
        case .all: return "--all-namespaces"
        case .named(let name): return "-n \(try validate(name))"
        }
    }

    public static func podsCommand(_ namespace: KubernetesNamespace) throws -> String {
        "kubectl get pods \(try flag(namespace)) --no-headers 2>/dev/null"
    }

    public static func deploymentsCommand(_ namespace: KubernetesNamespace) throws -> String {
        "kubectl get deployments \(try flag(namespace)) --no-headers 2>/dev/null"
    }

    /// Noder är kluster-globala och tar därför INGEN namnrymd.
    public static func nodesCommand() -> String {
        "kubectl get nodes --no-headers 2>/dev/null"
    }

    public static func podLogsCommand(namespace: String, pod: String, tail: Int) throws -> String {
        let n = max(1, tail)
        return "kubectl -n \(try validate(namespace)) logs --tail \(n) \(try validate(pod)) 2>&1"
    }

    /// Händelser och orsak till att något inte startar — den vanligaste
    /// felsökningsvägen i Kubernetes.
    public static func describePodCommand(namespace: String, pod: String) throws -> String {
        "kubectl -n \(try validate(namespace)) describe pod \(try validate(pod)) 2>&1"
    }

    /// Heter `delete` för att det är vad som händer: en podd startas aldrig om,
    /// den ersätts. En podd UTAN controller kommer inte tillbaka.
    public static func deletePodCommand(namespace: String, pod: String) throws -> String {
        "kubectl -n \(try validate(namespace)) delete pod \(try validate(pod)) 2>&1"
    }

    /// Rullande omstart — ersätter poddarna en i taget utan avbrott.
    public static func restartDeploymentCommand(namespace: String, deployment: String) throws -> String {
        "kubectl -n \(try validate(namespace)) rollout restart deployment/\(try validate(deployment)) 2>&1"
    }

    /// `split(whereSeparator: isWhitespace)` och inte `split(separator: " ")`:
    /// kubectl kolumnjusterar med varierande antal mellanslag.
    static func fields(_ line: Substring, _ expected: Int) -> [String]? {
        let parts = line.split(whereSeparator: { $0 == " " || $0 == "\t" }).map(String.init)
        return parts.count >= expected ? parts : nil
    }

    /// Kolumnerna skiftar med `--all-namespaces`, så namnrymden följer med IN i
    /// parsningen — en podd vars namn råkar se ut som en namnrymd hade förstört
    /// en gissning ur fältantalet.
    public static func parsePods(_ output: String, namespace: KubernetesNamespace) -> [KubernetesPod] {
        let all = namespace == .all
        let needed = all ? 5 : 4
        return output.split(whereSeparator: { $0 == "\n" || $0 == "\r" }).compactMap { line in
            guard let f = fields(line, needed) else { return nil }
            let ns: String
            let rest: [String]
            if all {
                ns = f[0]
                rest = Array(f.dropFirst())
            } else {
                guard case .named(let name) = namespace else { return nil }
                ns = name
                rest = f
            }
            return KubernetesPod(
                namespace: ns, name: rest[0], ready: rest[1], status: rest[2], restarts: rest[3]
            )
        }
    }

    public static func parseDeployments(
        _ output: String, namespace: KubernetesNamespace
    ) -> [KubernetesDeployment] {
        let all = namespace == .all
        let needed = all ? 3 : 2
        return output.split(whereSeparator: { $0 == "\n" || $0 == "\r" }).compactMap { line in
            guard let f = fields(line, needed) else { return nil }
            let ns: String
            let rest: [String]
            if all {
                ns = f[0]
                rest = Array(f.dropFirst())
            } else {
                guard case .named(let name) = namespace else { return nil }
                ns = name
                rest = f
            }
            return KubernetesDeployment(namespace: ns, name: rest[0], ready: rest[1])
        }
    }

    /// `kubectl get nodes` ger NAME STATUS ROLES AGE VERSION. Versionen tas
    /// BAKIFRÅN eftersom ROLES kan innehålla flera kommaseparerade värden.
    public static func parseNodes(_ output: String) -> [KubernetesNode] {
        output.split(whereSeparator: { $0 == "\n" || $0 == "\r" }).compactMap { line in
            guard let f = fields(line, 5) else { return nil }
            return KubernetesNode(name: f[0], status: f[1], version: f[f.count - 1])
        }
    }

    public static func parseNamespaces(_ output: String) -> [String] {
        output.split(whereSeparator: { $0 == "\n" || $0 == "\r" }).compactMap { line in
            fields(line, 2)?.first
        }
    }
}
