# Ghost-mux WebAssembly (Wasm) Compilation Guide

Ghost-mux is built on GPUI and uses `gpui-component` as its component toolkit. While GPUI historically targets native desktop windowing and GPU backends, the [gpui-component](file:///Users/saranyadamo/Downloads/ghost-mux/patches/gpui-component) dependency includes web rendering support through its [story-web](file:///Users/saranyadamo/Downloads/ghost-mux/patches/gpui-component/crates/story-web) crate. This documents how Ghost-mux can be compiled to Wasm to run inside a web browser under client-server mode.

---

## High-Level Architecture

In a Wasm-enabled client-server setup, the browser runs the layout engine, widgets, and user events using GPUI's canvas-based WebGL/WebGPU renderer. Heavy work and native interactions are proxied to the headless server.

```mermaid
graph TD
    subgraph Browser (Wasm Client)
        GPUI[GPUI Canvas Renderer] --> Layout[Panel Layout Tree]
        Layout --> Tabs[UI Tabs / Editor]
        WS[WebSocket / Fetch Transport]
    end
    
    subgraph Remote Host (Headless Server)
        Server[Ghost-mux Server]
        PTY[PTY / Terminals]
        LSP[Language Servers]
        FS[Local Filesystem]
    end
    
    WS <-->|JSON-RPC & Streams| Server
    Server <--> PTY
    Server <--> LSP
    Server <--> FS
```

---

## Blockers and Refactoring Steps

To compile Ghost-mux to the `wasm32-unknown-unknown` target, several native dependencies and modules must be modified or feature-flagged.

### 1. Networking Adaptation
* **File Affected**: [src/remote_api.rs](file:///Users/saranyadamo/Downloads/ghost-mux/src/remote_api.rs)
* **Problem**: Currently uses synchronous `std::net::TcpStream` to issue JSON-RPC HTTP POST requests to the headless server. TCP connections are unsupported in browser Wasm.
* **Refactoring**:
  Introduce a conditional compilation block to switch the request client:
  ```rust
  #[cfg(target_arch = "wasm32")]
  pub async fn call_remote_api(server_url: &str, method: &str, params: Value) -> Result<Value, String> {
      // Use web-sys and Fetch API
      use wasm_bindgen::JsCast;
      use web_sys::{Request, RequestInit, Response};
      
      // Async fetch logic here...
  }
  
  #[cfg(not(target_arch = "wasm32"))]
  pub fn call_remote_api(server_url: &str, method: &str, params: Value) -> Result<Value, String> {
      // Existing synchronous TcpStream logic
  }
  ```

### 2. Terminal Emulation
* **Files Affected**: [src/terminal/mod.rs](file:///Users/saranyadamo/Downloads/ghost-mux/src/terminal/mod.rs), [Cargo.toml](file:///Users/saranyadamo/Downloads/ghost-mux/Cargo.toml)
* **Problem**: Ghost-mux links against `libghostty-vt-sys` (C/Zig binary wrapper) and uses `portable-pty` to spawn terminals locally, which fail compiling to Wasm.
* **Refactoring**:
  * Create a conditional terminal view.
  * In Wasm mode, instead of doing local emulation via `libghostty-vt`, establish a WebSocket connection directly to the server.
  * Render terminal streams in the browser. You can either build a GPUI rendering wrapper over a pure-rust terminal parser (like `alacritty_terminal`) or integrate with browser-native renderers (e.g., placing an `xterm.js` canvas overlays above GPUI).

### 3. macOS Cocoa WebView
* **File Affected**: [src/browser.rs](file:///Users/saranyadamo/Downloads/ghost-mux/src/browser.rs)
* **Problem**: Hosts `WKWebView` natively on macOS using Cocoa pointers (`objc` crate dependencies).
* **Refactoring**:
  Disable Cocoa browser handles when target is Wasm:
  ```rust
  #[cfg(target_arch = "wasm32")]
  pub struct WebViewHandle {
      // Wasm-compatible or disabled representation
  }
  ```
  For visual output inside the browser, the web client can render an standard HTML `<iframe>` on top of the GPUI canvas at the layout boundaries.

### 4. Font and Asset Loading
* **Files Affected**: [src/main.rs](file:///Users/saranyadamo/Downloads/ghost-mux/src/main.rs), [settings.yaml](file:///Users/saranyadamo/Downloads/ghost-mux/settings.yaml)
* **Problem**: The app queries standard OS directories for fonts. In the browser sandbox, system fonts are not directly accessible.
* **Refactoring**:
  Assets and fonts must be bundled or served remotely. In the web runner entrypoint (modeled after [story-web/src/lib.rs](file:///Users/saranyadamo/Downloads/ghost-mux/patches/gpui-component/crates/story-web/src/lib.rs#L40-L55)), include static byte slices of required fonts (e.g., `JetBrains Mono` and CJK fonts) and register them:
  ```rust
  let jetbrains_mono = include_bytes!("../fonts/JetBrainsMono-Regular.ttf");
  cx.text_system()
      .add_fonts(vec![Cow::Borrowed(jetbrains_mono.as_slice())])
      .expect("Failed to load fonts");
  ```

---

## Setting up a Web Runner Crate

To build and compile the frontend:

1. **Create a `web-client` sub-crate** (similar to `crates/story-web` in the patch directory).
2. **Add a `lib.rs`** exposing a `#[wasm_bindgen]` entrypoint:
   ```rust
   use wasm_bindgen::prelude::*;
   
   #[wasm_bindgen]
   pub fn run() -> Result<(), JsValue> {
       #[cfg(target_family = "wasm")]
       gpui_platform::web_init();
       
       let app = gpui_platform::single_threaded_web();
       app.run(|cx| {
           // Initialize settings, fonts, and open a GPUI window
       });
       Ok(())
   }
   ```
3. **Build the WASM binary**:
   ```bash
   rustup target add wasm32-unknown-unknown
   cargo install wasm-bindgen-cli
   cargo build --target wasm32-unknown-unknown --package web-client --release
   wasm-bindgen --target web --out-dir ./www/pkg ../target/wasm32-unknown-unknown/release/web_client.wasm
   ```
4. **Vite / Node server**: Host the generated Wasm file alongside a basic `index.html` referencing the JavaScript bootstrap launcher.
