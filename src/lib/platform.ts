// WKWebView puts "Macintosh" in its user agent. WebKitGTK and WebView2 do not.
export const isMac = navigator.userAgent.includes("Macintosh");
