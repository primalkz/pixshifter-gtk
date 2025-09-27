use glib::ControlFlow;
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, HeaderBar, Box as GtkBox, Orientation,
    ComboBoxText, SpinButton, Button, Label,
};
use std::cell::RefCell;
use std::process::Command;
use std::rc::Rc;
use std::time::Duration;
use glib::source::SourceId;

// Trait to allow setting the text of the status label from any closure/thread
trait SetText {
    fn set_text_safe(&self, text: &str);
}

impl SetText for Label {
    fn set_text_safe(&self, text: &str) {
        // Use glib::idle_add_local to ensure the UI update happens on the main thread
        let label = self.clone();
        let message = text.to_string();
        glib::idle_add_local(move || {
            label.set_text(&message);
            ControlFlow::Break // Run once
        });
    }
}

/// Get connected displays
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

/// Detect the *current* resolution of a display (W x H)
fn get_resolution(display: &str) -> Option<(u32, u32)> {
    let output = Command::new("xrandr")
        .arg("--current")
        .output()
        .ok()?
        .stdout;

    let text = String::from_utf8_lossy(&output);
    let mut lines = text.lines();

    // Look for the display's 'connected' line, which contains the active resolution.
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.starts_with(display) && trimmed.contains(" connected") {

            // Look for the resolution pattern "WIDTHxHEIGHT+X+Y" on the connected line.
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            for part in parts {
                if part.contains('x') && part.contains('+') {
                    // Extract WIDTHxHEIGHT from "WIDTHxHEIGHT+X+Y"
                    if let Some((res_part, _)) = part.split_once('+') {
                        // Split at 'x'
                        if let Some((w_str, h_str)) = res_part.split_once('x') {
                            if let (Ok(width), Ok(height)) = (w_str.parse::<u32>(), h_str.parse::<u32>()) {
                                return Some((width, height));
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback method: check the indented lines for the active resolution marked by *
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.contains('*') {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            for part in parts {
                if part.contains('x') {
                    let clean_part: String = part.chars()
                        .take_while(|c| c.is_ascii_digit() || *c == 'x')
                        .collect();

                    if let Some((w_str, h_str)) = clean_part.split_once('x') {
                        if let (Ok(width), Ok(height)) = (w_str.parse::<u32>(), h_str.parse::<u32>()) {
                            return Some((width, height));
                        }
                    }
                }
            }
        }
    }

    None
}

// -------------------------------------------------------------------
// HYBRID ATTEMPT: Uses fixed 2px FB + dynamic Transform
// -------------------------------------------------------------------

/// Resets the display to the driver's default settings, clearing all transforms and framebuffers.
fn reset_display(display: &str, status_label: &Label) {
    status_label.set_text_safe(&format!("CMD: xrandr --output {} --auto", display));

    let result = Command::new("xrandr")
        .args(["--output", display, "--auto"])
        .spawn();

    if let Err(e) = result {
        let msg = format!("Failed to execute full reset (xrandr --auto) for {}: {}", display, e);
        eprintln!("{}", msg);
        status_label.set_text_safe(&msg);
    } else {
        status_label.set_text_safe(&format!("SUCCESS: {} fully RESET to driver's default (--auto).", display));
    }
}

/// Shifts using --transform matrix while forcing a stable, small framebuffer (--fb).
fn apply_shift(display: &str, amount: i32, mode: bool, status_label: &Label) {
    if let Some((w, h)) = get_resolution(display) {

        let x_shift = if mode { amount as f64 } else { 0.0 };
        let y_shift = if mode { amount as f64 } else { 0.0 };

        // 1. Calculate Transform Matrix values
        let tx = x_shift / w as f64;
        let ty = y_shift / h as f64;

        let transform_arg = format!("1,0,{:.6},0,1,{:.6},0,0,1", tx, ty);

        // 2. Calculate Framebuffer arguments (FIXED, SMALL OFFSET: +2px)
        let fixed_fb_offset = 2;
        let fb_w = w + fixed_fb_offset;
        let fb_h = h + fixed_fb_offset;
        let fb_arg = format!("{}x{}", fb_w, fb_h);

        // 3. Current resolution mode (to ensure the physical signal remains constant)
        let mode_arg = format!("{}x{}", w, h);


        status_label.set_text_safe(&format!("CMD: xrandr --output {} --mode {} --fb {} --transform {}", display, mode_arg, fb_arg, transform_arg));

        let result = Command::new("xrandr")
            .args([
                "--output", display,
                "--mode", &mode_arg,
                "--fb", &fb_arg,
                "--transform", &transform_arg,
            ])
            .spawn();

        if let Err(e) = result {
            let msg = format!("Failed to run xrandr command for {}: {}", display, e);
            eprintln!("{}", msg);
            status_label.set_text_safe(&msg);
        } else {
             let action = if mode { "TRANSFORMED" } else { "BASE (Identity)" };
             status_label.set_text_safe(&format!("SUCCESS: {} set to {} (shift {}px, FB {}x{}).", display, action, x_shift, fb_w, fb_h));
        }

    } else {
        let msg = format!("Could not detect resolution for {}. Shift FAILED.", display);
        eprintln!("{}", msg);
        status_label.set_text_safe(&msg);
    }
}
// -------------------------------------------------------------------

fn main() {
    let app = Application::builder()
        .application_id("com.example.PixelShift")
        .build();

    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    // Window Setup
    let window = ApplicationWindow::builder()
        .application(app)
        .title("PixelShift X11 Hybrid (Final Attempt)")
        .default_width(420)
        .default_height(320)
        .build();

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

    // Dropdown (Display Selection)
    let combo = ComboBoxText::new();
    let displays = get_connected_displays();
    for d in &displays {
        combo.append_text(d);
    }
    if let Some(_first) = displays.first() {
        combo.set_active(Some(0));
    }
    vbox.append(&Label::new(Some("Select Display:")));
    vbox.append(&combo);

    // Shift Amount Spin Button (Integer pixels)
    let shift_spin = SpinButton::with_range(1.0, 20.0, 1.0);
    shift_spin.set_value(1.0);
    shift_spin.set_digits(0);
    vbox.append(&Label::new(Some("Shift Amount (px) for Transform Matrix (1-20):")));
    vbox.append(&shift_spin);

    // Interval Spin Button
    let interval_spin = SpinButton::with_range(10.0, 600.0, 10.0);
    interval_spin.set_value(60.0);
    vbox.append(&Label::new(Some("Interval (seconds):")));
    vbox.append(&interval_spin);

    // Buttons
    let button_box = GtkBox::new(Orientation::Horizontal, 12);
    let shift_once_button = Button::with_label("Transform Once");
    let start_button = Button::with_label("Start Auto Transform");
    let stop_button = Button::with_label("Stop & Reset");
    button_box.append(&shift_once_button);
    button_box.append(&start_button);
    button_box.append(&stop_button);
    vbox.append(&button_box);

    // Status Label
    let status_label = Label::new(Some("Status: Ready. X11 Mode Set confirmed to be the root cause."));
    status_label.set_halign(gtk4::Align::Start);
    status_label.set_wrap(true);
    vbox.append(&status_label);
    let status_label_rc = Rc::new(status_label);

    // State Management
    let running_id: Rc<RefCell<Option<SourceId>>> = Rc::new(RefCell::new(None));
    let is_shifted: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));

    // Shift Once Handler
    shift_once_button.connect_clicked(gtk4::glib::clone!(@weak combo, @weak shift_spin, @strong status_label_rc => move |_| {
        if let Some(gs) = combo.active_text() {
            let display = gs.to_string();
            let amount = shift_spin.value_as_int();

            // 1. Shift to offset (+X+X)
            apply_shift(&display, amount, true, &status_label_rc);

            // 2. Set a single timeout to reset the shift to base (Identity) after 2 seconds
            glib::timeout_add_local(
                Duration::from_secs(2),
                gtk4::glib::clone!(@strong display, @strong status_label_rc => @default-return ControlFlow::Break, move || {
                    // Reset by applying the Identity matrix (0px shift) while keeping the fixed FB size
                    apply_shift(&display, 0, false, &status_label_rc);
                    ControlFlow::Break
                })
            );
        }
    }));

    // Start Auto Shift Handler
    start_button.connect_clicked(gtk4::glib::clone!(@weak combo, @weak shift_spin, @weak interval_spin, @strong running_id, @strong is_shifted, @strong status_label_rc => move |btn| {
        if running_id.borrow().is_some() { return; }

        if let Some(gs) = combo.active_text() {
            let display_name = gs.to_string();
            let interval = interval_spin.value_as_int().max(1) as u64;

            *is_shifted.borrow_mut() = false;
            status_label_rc.set_text_safe(&format!("Auto-transform STARTING for {} every {}s. Initializing to BASE (Identity).", display_name, interval));

            let sid = glib::timeout_add_local(
                Duration::from_secs(interval),
                gtk4::glib::clone!(@strong display_name, @weak shift_spin, @strong is_shifted, @strong status_label_rc => @default-return ControlFlow::Break, move || {
                    let amount = shift_spin.value_as_int();
                    let mut shifted = is_shifted.borrow_mut();

                    // Toggle the state (BASE <-> TRANSFORMED)
                    if *shifted {
                        // Current state is transformed, so reset to base (Identity, 0px shift)
                        apply_shift(&display_name, 0, false, &status_label_rc);
                        *shifted = false;
                    } else {
                        // Current state is base, so transform (using amount px shift)
                        apply_shift(&display_name, amount, true, &status_label_rc);
                        *shifted = true;
                    }
                    ControlFlow::Continue
                })
            );

            *running_id.borrow_mut() = Some(sid);
            btn.set_sensitive(false);
        } else {
             status_label_rc.set_text_safe("ERROR: No display selected to start auto-transform.");
        }
    }));

    // Stop & Reset Handler (Uses the clean reset_display function)
    stop_button.connect_clicked(gtk4::glib::clone!(@strong running_id, @strong is_shifted, @weak combo, @weak start_button, @strong status_label_rc => move |_| {
        if let Some(id) = running_id.borrow_mut().take() {
            id.remove();
        }

        // Ensure the display is fully reset to native resolution and default state
        if let Some(gs) = combo.active_text() {
            let display = gs.to_string();
            reset_display(&display, &status_label_rc);
        }
        *is_shifted.borrow_mut() = false;

        start_button.set_sensitive(true);
        status_label_rc.set_text_safe("Status: Auto-transform STOPPED and display fully RESET via --auto.");
    }));


    window.set_child(Some(&vbox));
    window.show();
}

