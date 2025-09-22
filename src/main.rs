use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, HeaderBar, Box as GtkBox, Orientation,
    ComboBoxText, SpinButton, Button, Label,
};
use std::process::Command;
use std::thread;
use std::time::Duration;

fn get_connected_displays() -> Vec<String> {
    let output = Command::new("xrandr")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();

    output
        .lines()
        .filter(|line| line.contains(" connected"))
        .filter_map(|line| line.split_whitespace().next().map(|s| s.to_string()))
        .collect()
}

fn shift_display(display: &str, amount: i32, duration: u64) {
    // Shift by amount
    let _ = Command::new("xrandr")
        .args([
            "--output",
            display,
            "--transform",
            &format!("1,0,{},0,1,{},0,0,1", amount, amount),
        ])
        .status();

    // Reset after duration
    let display_clone = display.to_string();
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(duration));
        let _ = Command::new("xrandr")
            .args([
                "--output",
                &display_clone,
                "--transform",
                "1,0,0,0,1,0,0,0,1",
            ])
            .status();
    });
}

fn main() {
    let app = Application::builder()
        .application_id("com.example.PixelShift")
        .build();

    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    // Window
    let window = ApplicationWindow::builder()
        .application(app)
        .title("PixelShift OLED Saver")
        .default_width(420)
        .default_height(240)
        .build();

    // HeaderBar
    let header = HeaderBar::builder()
        .title_widget(&Label::new(Some("PixelShift OLED Saver")))
        .show_title_buttons(true)
        .build();

    window.set_titlebar(Some(&header));

    // Layout
    let vbox = GtkBox::new(Orientation::Vertical, 12);
    vbox.set_margin_top(20);
    vbox.set_margin_bottom(20);
    vbox.set_margin_start(20);
    vbox.set_margin_end(20);

    // Dropdown for display selection
    let combo = ComboBoxText::new();
    let displays = get_connected_displays();
    for d in &displays {
        combo.append_text(d);
    }
    if let Some(first) = displays.first() {
        combo.set_active_id(Some(first.as_str()));
    }
    vbox.append(&Label::new(Some("Select Display:")));
    vbox.append(&combo);

    // Shift spin
    let shift_spin = SpinButton::with_range(1.0, 5.0, 1.0);
    shift_spin.set_value(1.0);
    vbox.append(&Label::new(Some("Shift Amount (px):")));
    vbox.append(&shift_spin);

    // Interval spin
    let interval_spin = SpinButton::with_range(10.0, 600.0, 10.0);
    interval_spin.set_value(60.0);
    vbox.append(&Label::new(Some("Interval (seconds):")));
    vbox.append(&interval_spin);

    // Buttons
    let button_box = GtkBox::new(Orientation::Horizontal, 12);
    let shift_button = Button::with_label("Shift Once");
    let start_button = Button::with_label("Start Auto Shift");
    let stop_button = Button::with_label("Stop");

    button_box.append(&shift_button);
    button_box.append(&start_button);
    button_box.append(&stop_button);
    vbox.append(&button_box);

    // Connect Shift Once
    shift_button.connect_clicked(glib::clone!(@weak combo, @weak shift_spin => move |_| {
        if let Some(gs) = combo.active_text() {
            let display = gs.to_string();
            let amount = shift_spin.value_as_int();
            shift_display(&display, amount, 1);
        } else {
            eprintln!("No display selected");
        }
    }));

    // TODO: hook start/stop using glib::timeout_add_local
    window.set_child(Some(&vbox));
    window.show();
}

