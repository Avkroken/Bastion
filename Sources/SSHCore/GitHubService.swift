import Foundation

// GitHub via `gh` över SSH. Port av LinuxApp/integrations/src/github.rs.
//
// `gh` på värden och INTE GitHubs API härifrån. Frågan vyn svarar på är "vad
// händer med koden på DEN HÄR servern" — en byggserver eller deploy-värd har
// utcheckade repon och ett inloggat `gh`. Att prata med api.github.com hade
// krävt en token att lagra, och svaret hade handlat om ett konto.

public enum GitHubError: Error, Sendable, Equatable {
    case invalidRepoPath(String)
}

public struct GitHubRun: Sendable, Equatable {
    public let name: String
    /// `completed`, `in_progress`, `queued`.
    public let status: String
    /// `success`, `failure`, `cancelled` — tom medan körningen pågår.
    public let conclusion: String
    public let branch: String

    public init(name: String, status: String, conclusion: String, branch: String) {
        self.name = name
        self.status = status
        self.conclusion = conclusion
        self.branch = branch
    }

    public var isRunning: Bool { status != "completed" }

    /// En PÅGÅENDE körning har ingen slutsats, och att läsa tom slutsats som
    /// "inte misslyckad" vore rätt av fel skäl — den kan fortfarande falla.
    /// Statusen frågas därför först.
    public var failed: Bool {
        !isRunning && ["failure", "timed_out", "startup_failure"].contains(conclusion)
    }
}

public struct GitHubPullRequest: Sendable, Equatable {
    public let number: Int
    public let title: String
    public let isDraft: Bool
    /// `CLEAN`, `BLOCKED`, `BEHIND`, `DIRTY`, `UNKNOWN`.
    public let mergeable: String

    public init(number: Int, title: String, isDraft: Bool, mergeable: String) {
        self.number = number
        self.title = title
        self.isDraft = isDraft
        self.mergeable = mergeable
    }

    /// `BLOCKED` (en check är inte grön ännu) och `DIRTY` (konflikt) är
    /// väsensskilda och kräver olika åtgärder. Orsaken behålls därför i
    /// `mergeable` i stället för att slås ihop till "kan inte mergas".
    public var isReady: Bool { mergeable == "CLEAN" && !isDraft }
}

public enum GitHubService {
    /// Katalognamn är godtyckliga och får innehålla mellanslag. Inom enkla
    /// citattecken är varje tecken utom `'` literalt i POSIX sh, så det räcker
    /// att avvisa apostrof — en teckenlista hade avvisat giltiga sökvägar.
    static func quotePath(_ path: String) throws -> String {
        guard !path.trimmingCharacters(in: .whitespaces).isEmpty, !path.contains("'") else {
            throw GitHubError.invalidRepoPath(path)
        }
        return "'\(path)'"
    }

    /// `&&` och inte `;`: misslyckas katalogbytet ska `gh` inte köras alls.
    /// Med semikolon hade kommandot svarat om vilket repo som råkar ligga i
    /// hemkatalogen — ett svar som ser giltigt ut och gäller fel sak.
    static func inRepo(_ path: String, _ args: String) throws -> String {
        "cd \(try quotePath(path)) && gh \(args) 2>&1"
    }

    public static func runsCommand(repoPath: String, limit: Int) throws -> String {
        let n = min(50, max(1, limit))
        return try inRepo(repoPath, "run list --limit \(n) --json name,status,conclusion,headBranch")
    }

    public static func pullRequestsCommand(repoPath: String, limit: Int) throws -> String {
        let n = min(50, max(1, limit))
        return try inRepo(repoPath, "pr list --limit \(n) --json number,title,isDraft,mergeStateStatus")
    }

    public static func authStatusCommand() -> String { "gh auth status 2>&1" }

    static func array(_ output: String) -> [[String: Any]] {
        guard let data = output.trimmingCharacters(in: .whitespacesAndNewlines).data(using: .utf8),
              let parsed = try? JSONSerialization.jsonObject(with: data),
              let list = parsed as? [[String: Any]]
        else { return [] }
        return list
    }

    public static func parseRuns(_ output: String) -> [GitHubRun] {
        array(output).compactMap { item in
            let name = item["name"] as? String ?? ""
            guard !name.isEmpty else { return nil }
            return GitHubRun(
                name: name,
                status: item["status"] as? String ?? "",
                conclusion: item["conclusion"] as? String ?? "",
                branch: item["headBranch"] as? String ?? ""
            )
        }
    }

    public static func parsePullRequests(_ output: String) -> [GitHubPullRequest] {
        array(output).compactMap { item in
            guard let number = item["number"] as? Int else { return nil }
            return GitHubPullRequest(
                number: number,
                title: item["title"] as? String ?? "",
                isDraft: item["isDraft"] as? Bool ?? false,
                mergeable: item["mergeStateStatus"] as? String ?? ""
            )
        }
    }

    /// Att leta efter FRAMGÅNGSraden och inte efter felord är avsiktligt:
    /// felmeddelandena varierar mellan gh-versioner, framgångsraden har varit
    /// stabil.
    public static func isAuthenticated(_ output: String) -> Bool {
        output.contains("Logged in to")
    }

    // MARK: - Körning över SSH

    public static func runs(
        repoPath: String, limit: Int = 20, over session: SSHSession
    ) async throws -> [GitHubRun] {
        parseRuns(try await session.run(try runsCommand(repoPath: repoPath, limit: limit)))
    }

    public static func pullRequests(
        repoPath: String, limit: Int = 20, over session: SSHSession
    ) async throws -> [GitHubPullRequest] {
        parsePullRequests(try await session.run(try pullRequestsCommand(repoPath: repoPath, limit: limit)))
    }

    public static func authStatus(over session: SSHSession) async throws -> Bool {
        isAuthenticated(try await session.run(authStatusCommand()))
    }
}
