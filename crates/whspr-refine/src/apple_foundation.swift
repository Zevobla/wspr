// Swift shim over Apple's on-device Foundation Models LLM (macOS 26+).
//
// whspr's Rust `AppleFoundation` refiner calls these @_cdecl entry points over
// the C ABI. FoundationModels is a Swift-only, macOS-26 framework, so this is
// the only way to reach it from Rust. It is weak-linked and every call is
// guarded by `#available(macOS 26, *)`, so a binary built against the macOS-26
// SDK still loads and runs on macOS 14-25 — the model just reports unavailable
// there and the refiner option stays hidden.
//
// Built by whspr-refine's build.rs with a swift.org 6.3.2 toolchain against the
// macOS-26 SDK at deployment target 14.0 (see flake.nix / the
// apple-native-shim-build note). Nothing to bundle: the Swift runtime ships in
// /usr/lib/swift on every macOS since 10.14.4.

import Foundation
import FoundationModels

// Availability of the on-device model, as a C-friendly code:
//   1  = available (macOS 26+, Apple Intelligence on, model present)
//   0  = OS is new enough but the model is unavailable (AI off / unsupported
//        device / model not yet downloaded)
//  -1  = OS older than macOS 26 (framework absent)
@_cdecl("whspr_fm_available")
public func whspr_fm_available() -> Int32 {
    guard #available(macOS 26.0, *) else { return -1 }
    switch SystemLanguageModel.default.availability {
    case .available: return 1
    default: return 0
    }
}

// Refine `promptC` under system `instructionsC` with the on-device model.
// Returns a malloc'd UTF-8 C string (free with whspr_fm_string_free) on success,
// or nil plus a malloc'd message written to *errOut (also caller-freed).
@_cdecl("whspr_fm_refine")
public func whspr_fm_refine(
    _ instructionsC: UnsafePointer<CChar>,
    _ promptC: UnsafePointer<CChar>,
    _ errOut: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>?
) -> UnsafeMutablePointer<CChar>? {
    errOut?.pointee = nil
    guard #available(macOS 26.0, *) else {
        errOut?.pointee = strdup("Foundation Models requires macOS 26 or later")
        return nil
    }

    let instructions = String(cString: instructionsC)
    let prompt = String(cString: promptC)

    // Foundation Models is async; drive it to completion synchronously so the
    // caller (a Rust spawn_blocking worker, never the main thread) just blocks.
    let sem = DispatchSemaphore(value: 0)
    nonisolated(unsafe) var outText: String? = nil
    nonisolated(unsafe) var outErr: String? = nil
    Task {
        do {
            let session = LanguageModelSession(instructions: instructions)
            let response = try await session.respond(to: prompt)
            outText = response.content
        } catch {
            outErr = String(describing: error)
        }
        sem.signal()
    }
    sem.wait()

    if let text = outText {
        return strdup(text)
    }
    errOut?.pointee = strdup(outErr ?? "Foundation Models produced no response")
    return nil
}

// Free a string returned by whspr_fm_refine (result or *errOut).
@_cdecl("whspr_fm_string_free")
public func whspr_fm_string_free(_ p: UnsafeMutablePointer<CChar>?) {
    if let p = p {
        free(p)
    }
}
