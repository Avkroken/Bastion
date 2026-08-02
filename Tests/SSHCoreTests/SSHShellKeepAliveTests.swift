import XCTest
@testable import SSHCore

/// `SSHShell.startKeepAlive` — täcker bara "håll NAT-mappningen varm"-delen
/// av ROADMAP.md "Anslutnings-resiliens" (se kommentaren i SSHShell.swift
/// för varför en fönsterändring, inte ett riktigt `keepalive@openssh.com`,
/// är mekanismen: swift-nio-ssh exponerar ingen generisk global request).
final class SSHShellKeepAliveTests: XCTestCase {
    func testStartKeepAliveSendsPeriodicWindowChangeRequests() async throws {
        let server = try LoopbackServer.start(password: "hunter2", trackWindowChanges: true)
        defer { server.shutdown() }

        let session = SSHSession(
            target: SSHTarget(host: "127.0.0.1", port: server.port, username: "tester"),
            auth: .password("hunter2"), knownHosts: KnownHosts(path: nil))
        try await session.connect()
        let shell = try await session.openShell(cols: 80, rows: 24)

        shell.startKeepAlive(interval: .milliseconds(20))
        try await Task.sleep(for: .milliseconds(150))
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
        try await Task.sleep(for: .milliseconds(80))
        shell.stopKeepAlive()
        let countAtStop = server.observedWindowChanges?.withLockedValue { $0.count } ?? 0

        // Om stopKeepAlive inte faktiskt avbryter Task:en skulle fler
        // fönsterändringar dyka upp under den här väntan också.
        try await Task.sleep(for: .milliseconds(100))
        let countAfterWait = server.observedWindowChanges?.withLockedValue { $0.count } ?? 0

        shell.close()
        await session.close()

        // <= 1 i stället för strikt likhet: en enda in-flight sändning kan
        // i teorin hinna slutföras om stopKeepAlive() råkar anropas exakt
        // när Task:en just vaknat men innan den hunnit kolla isCancelled
        // (kooperativ cancellation, inte en avbruten pågående operation) —
        // om den periodiska sändningen INTE stoppat skulle differensen vara
        // ~5 (100ms / 20ms), inte 0-1.
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
        try await Task.sleep(for: .milliseconds(100))
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
