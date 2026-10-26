import XCTest

final class Measurement: XCTestCase {
    let safari = XCUIApplication(bundleIdentifier: "com.apple.mobilesafari")
    let webapp = XCUIApplication(bundleIdentifier: "com.apple.webapp")
    let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")

    func capture(_ app: XCUIApplication, _ name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    func testInstall() throws {
        continueAfterFailure = false
        safari.activate()
        if safari.buttons["Continue"].waitForExistence(timeout: 3) { safari.buttons["Continue"].tap() }
        if safari.buttons["Start Browsing"].exists { safari.buttons["Start Browsing"].tap() }
        print("SAFARI TREE \(safari.debugDescription)")
        capture(safari, "safari-setup")
        let session = safari.buttons["Open session 1770 measurement"]
        XCTAssertTrue(session.waitForExistence(timeout: 60), safari.debugDescription)
        if safari.buttons["More"].exists { safari.buttons["More"].tap() }
        if !safari.buttons["Share"].exists {
            safari.coordinate(withNormalizedOffset: CGVector(dx: 0.88, dy: 0.96)).tap()
        }
        XCTAssertTrue(safari.buttons["Share"].waitForExistence(timeout: 5), safari.debugDescription)
        safari.buttons["Share"].tap()
        let home = safari.descendants(matching: .any).matching(NSPredicate(format: "label == 'Add to Home Screen'")).firstMatch
        for _ in 0..<3 { if home.exists && home.isHittable { break }; safari.swipeUp() }
        XCTAssertTrue(home.waitForExistence(timeout: 5), safari.debugDescription)
        home.tap()
        XCTAssertTrue(safari.buttons["Add"].waitForExistence(timeout: 5), safari.debugDescription)
        safari.buttons["Add"].tap()
        XCUIDevice.shared.press(.home)
        capture(springboard, "installed-icon")
    }

    func testMeasure() throws {
        continueAfterFailure = false
        for run in 1...5 {
            webapp.terminate()
            XCUIDevice.shared.press(.home)
            let icon = springboard.icons.matching(NSPredicate(format: "label CONTAINS 'TUICommander' OR label == 'TUIC'")).firstMatch
            XCTAssertTrue(icon.waitForExistence(timeout: 15), springboard.debugDescription)
            for _ in 0..<4 { if icon.isHittable { break }; springboard.swipeLeft() }
            icon.tap()
            XCTAssertTrue(webapp.wait(for: .runningForeground, timeout: 20))
            let session = webapp.buttons["Open session 1770 measurement"]
            if session.waitForExistence(timeout: 15) { session.tap() }
            let chat = webapp.descendants(matching: .any).matching(identifier: "Chat").firstMatch
            XCTAssertTrue(chat.waitForExistence(timeout: 30), webapp.debugDescription)
            chat.tap()
            waitReady("chat")
            capture(webapp, "first-open-\(run)")
            XCUIDevice.shared.press(.home)
            XCTAssertTrue(springboard.wait(for: .runningForeground, timeout: 10))
            icon.tap()
            XCTAssertTrue(webapp.wait(for: .runningForeground, timeout: 20))
            waitReady("resume")
            capture(webapp, "resume-\(run)")
            webapp.descendants(matching: .any).matching(identifier: "CLI").firstMatch.tap()
            waitReady("cli")
            chat.tap()
            waitReady("chat")
            webapp.descendants(matching: .any).matching(identifier: "CLI").firstMatch.tap()
            waitReady("cli")
            capture(webapp, "cli-roundtrip-\(run)")
        }
    }

    func waitReady(_ kind: String) {
        let ready = webapp.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", "SIM1770_READY_\(kind)_")).firstMatch
        XCTAssertTrue(ready.waitForExistence(timeout: 90), webapp.debugDescription)
    }
}
