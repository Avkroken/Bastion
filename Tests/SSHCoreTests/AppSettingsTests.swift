import XCTest
@testable import SSHCore

final class AppSettingsTests: XCTestCase {
    func testDefaultsAllEnabled() {
        let store = AppSettingsStore(path: nil)
        let toggles = store.current()
        XCTAssertTrue(toggles.showDocker)
        XCTAssertTrue(toggles.showSnippets)
        XCTAssertTrue(toggles.showCommandLibrary)
        XCTAssertTrue(toggles.showSFTPBrowser)
        XCTAssertTrue(toggles.showPortForward)
        XCTAssertTrue(toggles.showKeyDeploy)
    }

    func testUpdatePersistsInMemory() throws {
        let store = AppSettingsStore(path: nil)
        var toggles = store.current()
        toggles.showDocker = false
        try store.update(toggles)
        XCTAssertFalse(store.current().showDocker)
    }

    func testPersistsAcrossInstancesOnDisk() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let path = dir.appendingPathComponent("settings.json").path

        let store1 = AppSettingsStore(path: path)
        var toggles = store1.current()
        toggles.showDocker = false
        toggles.showKeyDeploy = false
        try store1.update(toggles)

        let store2 = AppSettingsStore(path: path)
        XCTAssertFalse(store2.current().showDocker)
        XCTAssertFalse(store2.current().showKeyDeploy)
        XCTAssertTrue(store2.current().showSnippets)
    }

    /// Skriver till en sökväg vars förälder redan är en VANLIG FIL (inte en
    /// katalog) — `createDirectory` misslyckas garanterat. `update` ska då
    /// kasta och lämna `current()` OFÖRÄNDRAD, inte tyst "spara" och sen
    /// glömma det vid nästa omstart.
    func testUpdateRollsBackOnPersistFailure() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let blockingFile = dir.appendingPathComponent("not-a-directory")
        try Data().write(to: blockingFile)
        let path = blockingFile.appendingPathComponent("settings.json").path

        let store = AppSettingsStore(path: path)
        let before = store.current()
        var toggles = before
        toggles.showDocker = false

        XCTAssertThrowsError(try store.update(toggles))
        XCTAssertEqual(store.current(), before, "misslyckad persist ska INTE lämna kvar det nya värdet i minnet")
    }

    func testMissingFileFallsBackToDefaults() {
        let path = FileManager.default.temporaryDirectory
            .appendingPathComponent("nonexistent-\(UUID().uuidString).json").path
        let store = AppSettingsStore(path: path)
        XCTAssertEqual(store.current(), FeatureToggles())
    }
}
