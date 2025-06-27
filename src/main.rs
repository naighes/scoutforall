mod constants;
mod r#match;
mod menu;
mod ops;
mod pdf;
mod set;
mod structs;
mod substitution;
mod team;
mod util;

mod errors;
mod io;
mod shapes;

#[cfg(test)]
mod tests;

fn main() {
    menu::run_menu();
}
