fn main() {
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("assets/icon.ico");
    resource.compile().expect("compile the icon resource");
}
