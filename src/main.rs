//! pichouse — a Picasa-like photo library GUI application for Linux.

mod version;

fn main() {
    // M0 scaffolding. The GTK4 UI is added in a later milestone.
    println!("pichouse {}", version::VERSION);
}
