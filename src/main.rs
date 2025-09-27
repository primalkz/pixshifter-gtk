use glib::ControlFlow;
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, HeaderBar, Box as GtkBox, Orientation,
    ComboBoxText, SpinButton, Button, Label, Switch,
};
use std::cell::RefCell;
use std::process::Command;
use std::rc::Rc;
use std::time::Duration;
use glib::source::SourceId;

// Enhanced trait for thread-safe UI updates
trait SetTextSafe {
    fn set_text_safe(&self, text: &str);
    fn append_text_safe(&self, text: &str);
}

impl SetTextSafe for Label {
    fn set_text_safe(&self, text: &str) {
        let label = self.clone();
        let message = text.to_string();
        glib::idle_add_local(move || {
            label.set_text(&message);
            ControlFlow::Break
        });
    }

    fn append_text_safe(&self, text: &str) {
        let label = self.clone();
        let message = text.to_string();
        glib::idle_add_local(move || {
            let current = label.text();
            label.set_text(&format!("{}\n{}", current, message));
            ControlFlow::Break
        });
    }
}

#[derive(Debug, Clone)]
struct DisplayInfo {
    name: String,
    width: u32,
    height: u32,
    refresh_rate: f64,
    is_primary: bool,
}

/// Enhanced display detection with better parsing
fn get_connected_displays() -> Vec<DisplayInfo> {
    let output = match Command::new("xrandr").arg("--query").output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
        Err(_) => return Vec::new(),
    };

    let mut displays = Vec::new();
    
    for line in output.lines() {
        if line.contains(" connected") && !line.contains("disconnected") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(name) = parts.first() {
                let name = name.to_string();
                let is_primary = line.contains("primary");
                
                // Find current resolution and refresh rate
                if let Some((width, height, refresh_rate)) = parse_current_mode(&output, &name) {
                    displays.push(DisplayInfo {
                        name: name.clone(),
                        width,
                        height,
                        refresh_rate,
                        is_primary,
                    });
                }
            }
        }
    }
    
    displays
}

fn parse_current_mode(xrandr_output: &str, display_name: &str) -> Option<(u32, u32, f64)> {
    let lines: Vec<&str> = xrandr_output.lines().collect();
    let mut found_display = false;
    
    for line in lines {
        if line.starts_with(display_name) && line.contains("connected") {
            found_display = true;
            
            // Try to find resolution in the connected line first
            let parts: Vec<&str> = line.split_whitespace().collect();
            for part in parts {
                if part.contains('x') && part.contains('+') {
                    if let Some((res_part, _)) = part.split_once('+') {
                        if let Some((w_str, h_str)) = res_part.split_once('x') {
                            if let (Ok(width), Ok(height)) = (w_str.parse::<u32>(), h_str.parse::<u32>()) {
                                return Some((width, height, 60.0)); // Default refresh rate
                            }
                        }
                    }
                }
            }
            continue;
        }
        
        if found_display && line.trim().starts_with(char::is_numeric) {
            // This is a mode line for our display
            if line.contains('*') && line.contains('+') {
                // Current active mode
                let parts: Vec<&str> = line.trim().split_whitespace().collect();
                if let Some(mode_str) = parts.first() {
                    if let Some((w_str, h_str)) = mode_str.split_once('x') {
                        if let (Ok(width), Ok(height)) = (w_str.parse::<u32>(), h_str.parse::<u32>()) {
                            // Try to extract refresh rate
                            let refresh_rate = parts.iter()
                                .find(|p| p.contains('*'))
                                .and_then(|p| p.trim_end_matches('*').trim_end_matches('+').parse().ok())
                                .unwrap_or(60.0);
                            return Some((width, height, refresh_rate));
                        }
                    }
                }
            }
        } else if found_display && !line.starts_with(' ') && !line.starts_with('\t') {
            // We've moved to another display
            break;
        }
    }
    
    None
}

/// Safe pixel shift using only panning (no transform matrices or framebuffer changes)
fn apply_pixel_shift_panning(display: &DisplayInfo, x_offset: i32, y_offset: i32, status_label: &Label) -> bool {
    // Simple panning - just specify the offset
    let panning_spec = format!("{}x{}+{}+{}", 
        display.width, display.height, x_offset, y_offset);
    
    status_label.set_text_safe(&format!("Applying panning: xrandr --output {} --panning {}", 
        display.name, panning_spec));

    let result = Command::new("xrandr")
        .args(["--output", &display.name, "--panning", &panning_spec])
        .output();

    match result {
        Ok(output) => {
            if output.status.success() {
                status_label.set_text_safe(&format!("✓ Panning applied: +{}+{}", x_offset, y_offset));
                true
            } else {
                let err = String::from_utf8_lossy(&output.stderr);
                status_label.set_text_safe(&format!("✗ Panning failed: {}", err));
                false
            }
        }
        Err(e) => {
            status_label.set_text_safe(&format!("✗ Command failed: {}", e));
            false
        }
    }
}

/// Alternative method using CRTC position changes (fixed format)
fn apply_pixel_shift_position(display: &DisplayInfo, x_offset: i32, y_offset: i32, status_label: &Label) -> bool {
    // Format position correctly - xrandr expects "x+y" format, handle negatives properly
    let pos_str = if x_offset >= 0 && y_offset >= 0 {
        format!("{}+{}", x_offset, y_offset)
    } else if x_offset < 0 && y_offset >= 0 {
        format!("{:+}+{}", x_offset, y_offset)  // This handles negative x
    } else if x_offset >= 0 && y_offset < 0 {
        format!("{}+{:+}", x_offset, y_offset)  // This handles negative y
    } else {
        format!("{:+}{:+}", x_offset, y_offset)  // Both negative
    };
    
    status_label.set_text_safe(&format!("Applying position shift: xrandr --output {} --pos {}", 
        display.name, pos_str));

    let result = Command::new("xrandr")
        .args([
            "--output", &display.name,
            "--pos", &pos_str
        ])
        .output();

    match result {
        Ok(output) => {
            if output.status.success() {
                status_label.set_text_safe(&format!("✓ Position shift applied: {}", pos_str));
                true
            } else {
                let err = String::from_utf8_lossy(&output.stderr);
                status_label.set_text_safe(&format!("✗ Position shift failed: {}", err));
                false
            }
        }
        Err(e) => {
            status_label.set_text_safe(&format!("✗ Command failed: {}", e));
            false
        }
    }
}

/// New method: Use transform matrix without framebuffer changes (most stable)
fn apply_pixel_shift_transform(display: &DisplayInfo, x_offset: i32, y_offset: i32, status_label: &Label) -> bool {
    // Calculate transform values as ratios (more precise than small pixel values)
    let tx = x_offset as f64 / display.width as f64;
    let ty = y_offset as f64 / display.height as f64;
    
    // Create transform matrix: translation only
    let transform_str = format!("1,0,{:.6},0,1,{:.6},0,0,1", tx, ty);
    
    status_label.set_text_safe(&format!("Applying transform shift: xrandr --output {} --transform {}", 
        display.name, transform_str));

    let result = Command::new("xrandr")
        .args([
            "--output", &display.name,
            "--transform", &transform_str
        ])
        .output();

    match result {
        Ok(output) => {
            if output.status.success() {
                status_label.set_text_safe(&format!("✓ Transform applied: {}px offset", 
                    if x_offset != 0 { x_offset } else { y_offset }));
                true
            } else {
                let err = String::from_utf8_lossy(&output.stderr);
                status_label.set_text_safe(&format!("✗ Transform failed: {}", err));
                false
            }
        }
        Err(e) => {
            status_label.set_text_safe(&format!("✗ Command failed: {}", e));
            false
        }
    }
}

/// Flicker-free panning with proper reset
fn apply_pixel_shift_panning_smooth(display: &DisplayInfo, x_offset: i32, y_offset: i32, status_label: &Label) -> bool {
    // Use a slightly larger panning area to avoid edge issues
    let panning_w = display.width + 10;
    let panning_h = display.height + 10;
    let panning_spec = format!("{}x{}+{}+{}", panning_w, panning_h, x_offset, y_offset);
    
    status_label.set_text_safe(&format!("Applying smooth panning: xrandr --output {} --panning {}", 
        display.name, panning_spec));

    let result = Command::new("xrandr")
        .args(["--output", &display.name, "--panning", &panning_spec])
        .output();

    match result {
        Ok(output) => {
            if output.status.success() {
                status_label.set_text_safe(&format!("✓ Smooth panning applied: +{}+{}", x_offset, y_offset));
                true
            } else {
                let err = String::from_utf8_lossy(&output.stderr);
                status_label.set_text_safe(&format!("✗ Smooth panning failed: {}", err));
                false
            }
        }
        Err(e) => {
            status_label.set_text_safe(&format!("✗ Command failed: {}", e));
            false
        }
    }
}

/// Reset display to normal state (enhanced)
fn reset_display_safe(display: &DisplayInfo, status_label: &Label) -> bool {
    // Try multiple reset methods in order of preference
    
    // Method 1: Reset transform matrix to identity
    let transform_reset = Command::new("xrandr")
        .args(["--output", &display.name, "--transform", "1,0,0,0,1,0,0,0,1"])
        .output();
    
    if let Ok(output) = transform_reset {
        if output.status.success() {
            status_label.set_text_safe(&format!("✓ Transform reset successful for {}", display.name));
            return true;
        }
    }
    
    // Method 2: Reset panning
    let panning_reset = Command::new("xrandr")
        .args(["--output", &display.name, "--panning", "0x0"])
        .output();
    
    if let Ok(output) = panning_reset {
        if output.status.success() {
            status_label.set_text_safe(&format!("✓ Panning reset successful for {}", display.name));
            return true;
        }
    }
    
    // Method 3: Reset position
    let pos_reset = Command::new("xrandr")
        .args(["--output", &display.name, "--pos", "0x0"])
        .output();
    
    if let Ok(output) = pos_reset {
        if output.status.success() {
            status_label.set_text_safe(&format!("✓ Position reset successful for {}", display.name));
            return true;
        }
    }
    
    // Method 4: Full auto reset as fallback
    let auto_reset = Command::new("xrandr")
        .args(["--output", &display.name, "--auto"])
        .output();
        
    match auto_reset {
        Ok(output) if output.status.success() => {
            status_label.set_text_safe(&format!("✓ Auto reset successful for {}", display.name));
            true
        }
        _ => {
            status_label.set_text_safe(&format!("✗ All reset methods failed for {}", display.name));
            false
        }
    }
}

#[derive(Clone)]
struct ShiftPattern {
    positions: Vec<(i32, i32)>,
    current_index: usize,
}

impl ShiftPattern {
    fn new(shift_amount: i32) -> Self {
        // Create a circular pattern to minimize visible transitions
        let positions = vec![
            (0, 0),                    // Center
            (shift_amount, 0),         // Right
            (shift_amount, shift_amount), // Bottom-right
            (0, shift_amount),         // Bottom
            (-shift_amount, shift_amount), // Bottom-left
            (-shift_amount, 0),        // Left
            (-shift_amount, -shift_amount), // Top-left
            (0, -shift_amount),        // Top
            (shift_amount, -shift_amount), // Top-right
        ];
        
        Self {
            positions,
            current_index: 0,
        }
    }
    
    fn next(&mut self) -> (i32, i32) {
        let pos = self.positions[self.current_index];
        self.current_index = (self.current_index + 1) % self.positions.len();
        pos
    }
    
    fn reset(&mut self) {
        self.current_index = 0;
    }
}

fn main() {
    let app = Application::builder()
        .application_id("com.example.AdvancedPixelShift")
        .build();

    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Advanced OLED Pixel Shifter")
        .default_width(500)
        .default_height(400)
        .build();

    let header = HeaderBar::builder()
        .title_widget(&Label::new(Some("OLED Pixel Shifter v2")))
        .show_title_buttons(true)
        .build();
    window.set_titlebar(Some(&header));

    let vbox = GtkBox::new(Orientation::Vertical, 12);
    vbox.set_margin_top(20);
    vbox.set_margin_bottom(20);
    vbox.set_margin_start(20);
    vbox.set_margin_end(20);

    // Display selection
    let combo = ComboBoxText::new();
    let displays = get_connected_displays();
    for display in &displays {
        let label = if display.is_primary {
            format!("{} ({}x{}, {:.1}Hz) [PRIMARY]", display.name, display.width, display.height, display.refresh_rate)
        } else {
            format!("{} ({}x{}, {:.1}Hz)", display.name, display.width, display.height, display.refresh_rate)
        };
        combo.append_text(&label);
    }
    if !displays.is_empty() {
        combo.set_active(Some(0));
    }
    let displays = Rc::new(RefCell::new(displays));
    vbox.append(&Label::new(Some("Select Display:")));
    vbox.append(&combo);

    // Shift amount
    let shift_spin = SpinButton::with_range(1.0, 10.0, 1.0);
    shift_spin.set_value(2.0);
    shift_spin.set_digits(0);
    vbox.append(&Label::new(Some("Shift Amount (pixels, 1-10):")));
    vbox.append(&shift_spin);

    // Method selection
    let method_combo = ComboBoxText::new();
    method_combo.append_text("Transform Matrix (Recommended)");
    method_combo.append_text("Smooth Panning");
    method_combo.append_text("Position Offset");
    method_combo.append_text("Basic Panning");
    method_combo.set_active(Some(0));
    vbox.append(&Label::new(Some("Shift Method:")));
    vbox.append(&method_combo);

    // Pattern mode
    let pattern_switch = Switch::new();
    pattern_switch.set_active(true);
    let pattern_box = GtkBox::new(Orientation::Horizontal, 6);
    pattern_box.append(&Label::new(Some("Use Circular Pattern:")));
    pattern_box.append(&pattern_switch);
    vbox.append(&pattern_box);

    // Interval
    let interval_spin = SpinButton::with_range(5.0, 300.0, 5.0);
    interval_spin.set_value(30.0);
    vbox.append(&Label::new(Some("Interval (seconds):")));
    vbox.append(&interval_spin);

    // Buttons
    let button_box = GtkBox::new(Orientation::Horizontal, 12);
    let test_button = Button::with_label("Test Shift");
    let start_button = Button::with_label("Start Auto-Shift");
    let stop_button = Button::with_label("Stop & Reset");
    button_box.append(&test_button);
    button_box.append(&start_button);
    button_box.append(&stop_button);
    vbox.append(&button_box);

    // Status
    let status_label = Label::new(Some("Ready. Select display and configure settings."));
    status_label.set_halign(gtk4::Align::Start);
    status_label.set_wrap(true);
    status_label.set_selectable(true);
    vbox.append(&status_label);

    // State management
    let running_id: Rc<RefCell<Option<SourceId>>> = Rc::new(RefCell::new(None));
    let shift_pattern: Rc<RefCell<Option<ShiftPattern>>> = Rc::new(RefCell::new(None));

    // Test shift handler
    test_button.connect_clicked(gtk4::glib::clone!(@weak combo, @weak shift_spin, @weak method_combo, @strong status_label, @strong displays => move |_| {
        if let Some(active_idx) = combo.active() {
            if let Some(display) = displays.borrow().get(active_idx as usize) {
                let shift_amount = shift_spin.value_as_int();
                let method_idx = method_combo.active().unwrap_or(0);
                
                status_label.set_text_safe("Testing pixel shift...");
                
                let success = match method_idx {
                    0 => apply_pixel_shift_transform(display, shift_amount, shift_amount, &status_label),
                    1 => apply_pixel_shift_panning_smooth(display, shift_amount, shift_amount, &status_label),
                    2 => apply_pixel_shift_position(display, shift_amount, shift_amount, &status_label),
                    3 => apply_pixel_shift_panning(display, shift_amount, shift_amount, &status_label),
                    _ => apply_pixel_shift_transform(display, shift_amount, shift_amount, &status_label),
                };
                
                if success {
                    // Reset after 3 seconds
                    glib::timeout_add_local(
                        Duration::from_secs(3),
                        gtk4::glib::clone!(@strong display, @strong status_label => @default-return ControlFlow::Break, move || {
                            reset_display_safe(&display, &status_label);
                            ControlFlow::Break
                        })
                    );
                }
            }
        }
    }));

    // Start auto-shift handler
    start_button.connect_clicked(gtk4::glib::clone!(@weak combo, @weak shift_spin, @weak method_combo, @weak pattern_switch, @weak interval_spin, @strong running_id, @strong shift_pattern, @strong status_label, @strong displays => move |btn| {
        if running_id.borrow().is_some() { return; }

        if let Some(active_idx) = combo.active() {
            if let Some(display) = displays.borrow().get(active_idx as usize) {
                let display = display.clone();
                let shift_amount = shift_spin.value_as_int();
                let method_idx = method_combo.active().unwrap_or(0);
                let use_pattern = pattern_switch.is_active();
                let interval_secs = interval_spin.value_as_int().max(5) as u64;
                
                // Initialize pattern
                if use_pattern {
                    *shift_pattern.borrow_mut() = Some(ShiftPattern::new(shift_amount));
                }
                
                status_label.set_text_safe(&format!("Starting auto-shift for {} every {}s", display.name, interval_secs));
                
                let sid = glib::timeout_add_local(
                    Duration::from_secs(interval_secs),
                    gtk4::glib::clone!(@strong display, @strong shift_pattern, @strong status_label => @default-return ControlFlow::Break, move || {
                        let (x_offset, y_offset) = if use_pattern {
                            if let Some(ref mut pattern) = shift_pattern.borrow_mut().as_mut() {
                                pattern.next()
                            } else {
                                (shift_amount, shift_amount)
                            }
                        } else {
                            // Simple alternating shift
                            static mut TOGGLE: bool = false;
                            unsafe {
                                TOGGLE = !TOGGLE;
                                if TOGGLE {
                                    (shift_amount, shift_amount)
                                } else {
                                    (0, 0)
                                }
                            }
                        };
                        
                        let _success = match method_idx {
                            0 => apply_pixel_shift_transform(&display, x_offset, y_offset, &status_label),
                            1 => apply_pixel_shift_panning_smooth(&display, x_offset, y_offset, &status_label),
                            2 => apply_pixel_shift_position(&display, x_offset, y_offset, &status_label),
                            3 => apply_pixel_shift_panning(&display, x_offset, y_offset, &status_label),
                            _ => apply_pixel_shift_transform(&display, x_offset, y_offset, &status_label),
                        };
                        
                        ControlFlow::Continue
                    })
                );
                
                *running_id.borrow_mut() = Some(sid);
                btn.set_sensitive(false);
            }
        }
    }));

    // Stop handler
    stop_button.connect_clicked(gtk4::glib::clone!(@weak combo, @weak start_button, @strong running_id, @strong shift_pattern, @strong status_label, @strong displays => move |_| {
        if let Some(id) = running_id.borrow_mut().take() {
            id.remove();
        }
        
        // Reset pattern
        if let Some(ref mut pattern) = shift_pattern.borrow_mut().as_mut() {
            pattern.reset();
        }
        
        if let Some(active_idx) = combo.active() {
            if let Some(display) = displays.borrow().get(active_idx as usize) {
                reset_display_safe(display, &status_label);
            }
        }
        
        start_button.set_sensitive(true);
        status_label.set_text_safe("Auto-shift stopped and display reset.");
    }));

    window.set_child(Some(&vbox));
    window.show();
}