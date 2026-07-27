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

    func testUpdatePersistsInMemory() {
        let store = AppSettingsStore(path: nil)
        var toggles = store.current()
        toggles.showDocker = false
        store.update(toggles)
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
        store1.update(toggles)

        let store2 = AppSettingsStore(path: path)
        XCTAssertFalse(store2.current().showDocker)
        XCTAssertFalse(store2.current().showKeyDeploy)
        XCTAssertTrue(store2.current().showSnippets)
    }

    func testMissingFileFallsBackToDefaults() {
        let path = FileManager.default.temporaryDirectory
            .appendingPathComponent("nonexistent-\(UUID().uuidString).json").path
        let store = AppSettingsStore(path: path)
        XCTAssertEqual(store.current(), FeatureToggles())
    }
}
