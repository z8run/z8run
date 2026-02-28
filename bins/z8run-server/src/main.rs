//! # z8run-server
//!
//! Server binary with embedded static frontend.
//! In Phase 1 it simply redirects to the main CLI.
//! In Phase 3+, this binary embeds the static files of the
//! React editor via rust-embed and serves them together with the API.

fn main() {
    // In Phase 1, the server is launched from the main CLI.
    // This binary will be used when the frontend is ready to be embedded.
    eprintln!("z8run-server: use 'z8run serve' to start the server");
    eprintln!("This binary will be activated when the frontend is integrated.");
    std::process::exit(1);
}
