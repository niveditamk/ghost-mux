#[cfg(target_os = "macos")]
mod macos {
    use objc::{msg_send, sel, sel_impl, runtime::{Object, Class}};
    use objc::declare::ClassDecl;
    use std::ffi::c_void;
    use std::sync::Once;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use gpui::{Window, Bounds, Pixels};

    #[link(name = "WebKit", kind = "framework")]
    extern "C" {}

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    pub struct CGPoint {
        pub x: f64,
        pub y: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    pub struct CGSize {
        pub width: f64,
        pub height: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    pub struct CGRect {
        pub origin: CGPoint,
        pub size: CGSize,
    }

    static REGISTER_DELEGATE: Once = Once::new();

    extern "C" fn did_finish_navigation(
        this: &Object,
        _cmd: objc::runtime::Sel,
        webview: *mut Object,
        _navigation: *mut Object,
    ) {
        unsafe {
            let url_obj: *mut Object = msg_send![webview, URL];
            if !url_obj.is_null() {
                let absolute_string: *mut Object = msg_send![url_obj, absoluteString];
                if !absolute_string.is_null() {
                    let utf8_str: *const u8 = msg_send![absolute_string, UTF8String];
                    if !utf8_str.is_null() {
                        let c_str = std::ffi::CStr::from_ptr(utf8_str as *const std::ffi::c_char);
                        if let Ok(url) = c_str.to_str() {
                            let callback_ptr = *this.get_ivar::<*const c_void>("rust_callback");
                            if !callback_ptr.is_null() {
                                let callback = &*(callback_ptr as *const Box<dyn Fn(String) + Send + Sync + 'static>);
                                callback(url.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    fn register_delegate_class() -> &'static Class {
        REGISTER_DELEGATE.call_once(|| {
            let superclass = Class::get("NSObject").unwrap();
            let mut decl = ClassDecl::new("BrowserNavigationDelegate", superclass).unwrap();
            decl.add_ivar::<*const c_void>("rust_callback");
            unsafe {
                decl.add_method(
                    sel!(webView:didFinishNavigation:),
                    did_finish_navigation as extern "C" fn(&Object, objc::runtime::Sel, *mut Object, *mut Object),
                );
            }
            decl.register();
        });
        Class::get("BrowserNavigationDelegate").unwrap()
    }

    pub struct WebViewHandle {
        webview: *mut Object,
        delegate: *mut Object,
    }

    unsafe impl Send for WebViewHandle {}
    unsafe impl Sync for WebViewHandle {}

    impl WebViewHandle {
        pub fn new(
            window: &Window,
            url: &str,
            on_url_changed: Box<dyn Fn(String) + Send + Sync + 'static>,
        ) -> Option<Self> {
            unsafe {
                let cls = Class::get("WKWebView")?;
                
                let frame = CGRect {
                    origin: CGPoint { x: 0.0, y: 0.0 },
                    size: CGSize { width: 100.0, height: 100.0 },
                };
                
                let webview: *mut Object = msg_send![cls, alloc];
                let webview: *mut Object = msg_send![webview, initWithFrame:frame];
                if webview.is_null() {
                    return None;
                }
                
                let delegate_cls = register_delegate_class();
                let delegate: *mut Object = msg_send![delegate_cls, alloc];
                let delegate: *mut Object = msg_send![delegate, init];
                if delegate.is_null() {
                    let _: () = msg_send![webview, release];
                    return None;
                }
                
                let box_callback = Box::new(on_url_changed);
                let raw_callback = Box::into_raw(box_callback) as *const c_void;
                (*delegate).set_ivar("rust_callback", raw_callback);
                
                let _: () = msg_send![webview, setNavigationDelegate:delegate];
                
                let rwh = raw_window_handle::HasWindowHandle::window_handle(window).ok()?;
                if let RawWindowHandle::AppKit(handle) = rwh.as_raw() {
                    let parent_view = handle.ns_view.as_ptr() as *mut Object;
                    let _: () = msg_send![parent_view, addSubview:webview];
                } else {
                    let _ = Box::from_raw(raw_callback as *mut Box<dyn Fn(String) + Send + Sync + 'static>);
                    let _: () = msg_send![delegate, release];
                    let _: () = msg_send![webview, release];
                    return None;
                }
                
                let handle = WebViewHandle { webview, delegate };
                handle.load_url(url);
                Some(handle)
            }
        }

        pub fn load_url(&self, url: &str) {
            unsafe {
                let url_str = if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("file://") {
                    url.to_string()
                } else {
                    format!("https://{}", url)
                };

                let bytes = url_str.as_bytes();
                let nsstring_cls = Class::get("NSString").unwrap();
                let ns_url_str: *mut Object = msg_send![nsstring_cls, alloc];
                let ns_url_str: *mut Object = msg_send![ns_url_str, initWithBytes:bytes.as_ptr() length:bytes.len() encoding:4]; // NSUTF8StringEncoding
                
                let nsurl_cls = Class::get("NSURL").unwrap();
                let nsurl: *mut Object = msg_send![nsurl_cls, URLWithString:ns_url_str];
                
                let nsurlrequest_cls = Class::get("NSURLRequest").unwrap();
                let request: *mut Object = msg_send![nsurlrequest_cls, alloc];
                let request: *mut Object = msg_send![request, initWithURL:nsurl];
                
                let _: *mut Object = msg_send![self.webview, loadRequest:request];
                
                let _: () = msg_send![ns_url_str, release];
                let _: () = msg_send![request, release];
            }
        }

        pub fn set_bounds(&self, window: &Window, bounds: Bounds<Pixels>) {
            unsafe {
                let window_height = (window.viewport_size().height / gpui::px(1.0)) as f64;
                let x = (bounds.origin.x / gpui::px(1.0)) as f64;
                let y = window_height - ((bounds.origin.y + bounds.size.height) / gpui::px(1.0)) as f64;
                let width = (bounds.size.width / gpui::px(1.0)) as f64;
                let height = (bounds.size.height / gpui::px(1.0)) as f64;
                
                let frame = CGRect {
                    origin: CGPoint { x: x as f64, y: y as f64 },
                    size: CGSize { width: width as f64, height: height as f64 },
                };
                
                let _: () = msg_send![self.webview, setFrame:frame];
            }
        }

        pub fn set_visible(&self, visible: bool) {
            unsafe {
                let _: () = msg_send![self.webview, setHidden:!visible];
            }
        }

        pub fn go_back(&self) {
            unsafe {
                let can_go_back: bool = msg_send![self.webview, canGoBack];
                if can_go_back {
                    let _: *mut Object = msg_send![self.webview, goBack];
                }
            }
        }

        pub fn go_forward(&self) {
            unsafe {
                let can_go_forward: bool = msg_send![self.webview, canGoForward];
                if can_go_forward {
                    let _: *mut Object = msg_send![self.webview, goForward];
                }
            }
        }

        pub fn reload(&self) {
            unsafe {
                let _: *mut Object = msg_send![self.webview, reload];
            }
        }
    }

    impl Drop for WebViewHandle {
        fn drop(&mut self) {
            unsafe {
                let _: () = msg_send![self.webview, removeFromSuperview];
                let callback_ptr = *(*self.delegate).get_ivar::<*const c_void>("rust_callback");
                if !callback_ptr.is_null() {
                    let _ = Box::from_raw(callback_ptr as *mut Box<dyn Fn(String) + Send + Sync + 'static>);
                }
                let _: () = msg_send![self.delegate, release];
                let _: () = msg_send![self.webview, release];
            }
        }
    }

    pub fn restore_gpui_focus(window: &gpui::Window) {
        unsafe {
            let rwh = raw_window_handle::HasWindowHandle::window_handle(window).ok();
            if let Some(rwh) = rwh {
                if let RawWindowHandle::AppKit(handle) = rwh.as_raw() {
                    let parent_view = handle.ns_view.as_ptr() as *mut Object;
                    if !parent_view.is_null() {
                        let window_obj: *mut Object = msg_send![parent_view, window];
                        if !window_obj.is_null() {
                            let current_first: *mut Object = msg_send![window_obj, firstResponder];
                            if current_first != parent_view {
                                let _: () = msg_send![window_obj, makeFirstResponder:parent_view];
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos::{WebViewHandle, restore_gpui_focus};

#[cfg(not(target_os = "macos"))]
pub struct WebViewHandle {}

#[cfg(not(target_os = "macos"))]
impl WebViewHandle {
    pub fn new(
        _window: &gpui::Window,
        _url: &str,
        _on_url_changed: Box<dyn Fn(String) + Send + Sync + 'static>,
    ) -> Option<Self> {
        None
    }
    pub fn load_url(&self, _url: &str) {}
    pub fn set_bounds(&self, _window: &gpui::Window, _bounds: gpui::Bounds<gpui::Pixels>) {}
    pub fn set_visible(&self, _visible: bool) {}
    pub fn go_back(&self) {}
    pub fn go_forward(&self) {}
    pub fn reload(&self) {}
}

#[cfg(not(target_os = "macos"))]
pub fn restore_gpui_focus(_window: &gpui::Window) {}

