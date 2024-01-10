mod macos;

pub fn create_menu() {
    #[cfg(target_os = "macos")]
    {
        macos::create_menu();
    }
}
