import XCTest
@testable import SSHCore

/// `SSHShell.startKeepAlive` — täcker bara "håll NAT-mappningen varm"-delen
/// av ROADMAP.md "Anslutnings-resiliens" (se kommentaren i SSHShell.swift
/// för varför en fönsterändring, inte ett riktigt `keepalive@openssh.com`,
/// är mekanismen: swift-nio-ssh exponerar ingen generisk global request).
///
/// Väntar via polling mot ett generöst timeout i stället för en fast
/// `Task.sleep` + exakt förväntat antal ticks — en riktig CI-runner under
/// last kan hinna ge scheduling-jitter i storleksordningen hundratals ms,
/// vilket gjorde en tidigare version av det här testet flakigt (0 sedda
/// fönsterändringar inom en 150ms-väntan med 20ms-intervall, på en riktig
/// macOS-runner — inte gissat, sett i faktisk CI-körning).
final class SSHShellKeepAliveTests: XCTestCase {
    private func waitUntil(
        timeout: Duration = .seconds(5), poll: Duration = .milliseconds(20),
        _ condition: () -> Bool
    ) async {
        let deadline = ContinuousClock.now + timeout
        while ContinuousClock.now < deadline {
            if condition() { return }
            try? await Task.sleep(for: poll)
        }
    }

    func testStartKeepAliveSendsPeriodicWindowChangeRequests() async throws {
        let server = try LoopbackServer.start(password: "hunter2", trackWindowChanges: true)
        defer { server.shutdown() }

        let session = SSHSession(
            target: SSHTarget(host: "127.0.0.1", port: server.port, username: "tester"),
            auth: .password("hunter2"), knownHosts: KnownHosts(path: nil))
        try await session.connect()
        let shell = try await session.openShell(cols: 80, rows: 24)

        shell.startKeepAlive(interval: .milliseconds(20))
        await waitUntil {
            (server.observedWindowChanges?.withLockedValue { $0.count } ?? 0) > 1
        }
        shell.stopKeepAlive()

        shell.close()
        await session.close()

        let seen = server.observedWindowChanges?.withLockedValue { $0 } ?? []
        XCTAssertGreaterThan(seen.count, 1, "väntade flera periodiska fönsterändringar, fick \(seen.count)")
        XCTAssertTrue(seen.allSatisfy { $0.cols == 80 && $0.rows == 24 }, "väntade oförändrad storlek, fick \(seen)")
    }

    func testStopKeepAliveActuallyStopsSending() async throws {
        let server = try LoopbackServer.start(password: "hunter2", trackWindowChanges: true)
        defer { server.shutdown() }

        let session = SSHSession(
            target: SSHTarget(host: "127.0.0.1", port: server.port, username: "tester"),
            auth: .password("hunter2"), knownHosts: KnownHosts(path: nil))
        try await session.connect()
        let shell = try await session.openShell(cols: 80, rows: 24)

        shell.startKeepAlive(interval: .milliseconds(20))
        // Vänta tills minst ett par sänts, så stopKeepAlive() faktiskt har
        // något pågående att avbryta — inte bara ett race mot startup.
        await waitUntil {
            (server.observedWindowChanges?.withLockedValue { $0.count } ?? 0) >= 2
        }
        shell.stopKeepAlive()
        let countAtStop = server.observedWindowChanges?.withLockedValue { $0.count } ?? 0

        // Om stopKeepAlive inte faktiskt avbryter Task:en skulle fler
        // fönsterändringar fortsätta dyka upp under den här väntan också —
        // generöst tilltagen (500ms) för att inte själv bli flakig.
        try? await Task.sleep(for: .milliseconds(500))
        let countAfterWait = server.observedWindowChanges?.withLockedValue { $0.count } ?? 0

        shell.close()
        await session.close()

        // <= 1 i stället för strikt likhet: en enda in-flight sändning kan
        // i teorin hinna slutföras om stopKeepAlive() råkar anropas exakt
        // när Task:en just vaknat men innan den hunnit kolla isCancelled
        // (kooperativ cancellation, inte en avbruten pågående operation) —
        // om den periodiska sändningen INTE stoppat skulle differensen vara
        // stor (25 ticks under 500ms med 20ms-intervall), inte 0-1.
        XCTAssertLessThanOrEqual(
            countAfterWait - countAtStop, 1,
            "stopKeepAlive stoppade inte den periodiska sändningen (\(countAtStop) -> \(countAfterWait))")
    }

    func testResizeUpdatesSizeUsedByKeepAlive() async throws {
        let server = try LoopbackServer.start(password: "hunter2", trackWindowChanges: true)
        defer { server.shutdown() }

        let session = SSHSession(
            target: SSHTarget(host: "127.0.0.1", port: server.port, username: "tester"),
            auth: .password("hunter2"), knownHosts: KnownHosts(path: nil))
        try await session.connect()
        let shell = try await session.openShell(cols: 80, rows: 24)

        shell.resize(cols: 200, rows: 60)
        shell.startKeepAlive(interval: .milliseconds(20))
        await waitUntil {
            (server.observedWindowChanges?.withLockedValue { $0.count } ?? 0) > 1
        }
        shell.stopKeepAlive()

        shell.close()
        await session.close()

        let seen = server.observedWindowChanges?.withLockedValue { $0 } ?? []
        // Den explicita resize() + minst en keepAlive-runda som återanvänder
        // den nya storleken.
        XCTAssertTrue(seen.contains { $0.cols == 200 && $0.rows == 60 }, "fick \(seen)")
        XCTAssertTrue(seen.allSatisfy { $0.cols == 200 && $0.rows == 60 }, "väntade bara 200x60 efter resize, fick \(seen)")
    }
}
