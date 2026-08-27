// Su Windows la console non deve comparire nelle build di release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    vanzakart_launcher_lib::run();
}
