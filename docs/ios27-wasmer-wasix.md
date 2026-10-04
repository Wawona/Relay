# iOS 27 Mode A Wasm: Wasmer WASIX

Packages stay bytecode on `/wasm/v1`. The execute engine changes with the OS.

| Host | Mode A execute |
|---|---|
| iOS / iPadOS 13 through 26 | Wasmtime Pulley |
| iOS / iPadOS 27+ with WasmerSDK linked | Wasmer WASIX in a hidden `WKWebView` (WebKit JIT and JSPI) |
| iOS / iPadOS 27+ without that SDK | Pulley. Fail closed. Do not pretend the WebView ran. |
| tvOS, watchOS, visionOS | Pulley |
| macOS, Linux | Wasmtime Cranelift. Not a second Wasmer engine. |
| Mode B iOS (tipa / Sileo) | Pulley until `MAP_JIT`. Wasmer WebKit is the Mode A path. |

Wasmer's iOS SDK does not ship a compiler. On iOS 27 and later it runs WASIX
inside WebKit, which is allowed to JIT. The public package is `WasmerSDK`
(`platforms: .iOS("27.0")` in `wasmerio/wasmer-sdk`). DNS, TCP, and files cross
the WebView with `callAsyncJavaScript` and `WKScriptMessageHandler`.

Reference: <https://wasmer.io/posts/wasmer-sdk-swift-ios-macos>

## What this binary does today

The installed iPhoneOS SDK is 26. `WWN_WASMER_IOS27` is off.
`wwn_wasmer_webkit_available()` returns 0. Relay resolves Mode A Wasm to
`wasm-pulley` even if `apple_os_major` is 27.

When an iPhoneOS 27 SDK build defines `WWN_WASMER_IOS27` and links `WasmerSDK`,
the same symbol returns 1 on OS 27 and later. `WWNRelay` then sets
`wasmer_webkit_linked` on the spec. `resolve_backend` returns
`wasm-wasmer-webkit`. `relay_start` calls `wwn_wasmer_webkit_start` instead of
Pulley.

The deployment target stays **13.0**. Do not raise it so the package will link.

## Selector

`RelaySpec.apple_os_major` is `NSProcessInfo.operatingSystemVersion.majorVersion`.
Absent means Pulley.

`RelaySpec.wasmer_webkit_linked` is true only when this process linked the SDK
and the running OS is 27 or newer.

Both must be true, and the artifact must be Mode A, and the platform must be
iOS or iPadOS.

## Still forbidden in the store IPA

Cranelift native codegen and `MAP_JIT`. Wasmer on iOS 13 through 26. A second
wasm catalog. Calling the WebView path a VM or a container.
