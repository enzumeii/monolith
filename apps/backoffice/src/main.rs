#[cfg(feature = "server")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    backoffice::run_server(std::env::args().any(|arg| arg == "--print-bind"))
}

#[cfg(not(feature = "server"))]
fn main() {
    dioxus::launch(backoffice::App);
}
