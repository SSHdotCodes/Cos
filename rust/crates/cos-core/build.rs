fn main() {
    // HIServices (which owns the AXUIElement APIs and kAX* constants) is a
    // nested framework of ApplicationServices, so the linker needs the nested
    // framework search path to resolve `-framework HIServices`.
    #[cfg(target_os = "macos")]
    {
        println!(
            "cargo:rustc-link-search=framework=/System/Library/Frameworks/ApplicationServices.framework/Frameworks"
        );
        println!("cargo:rustc-check-cfg=cfg(target_os)");
    }
}
