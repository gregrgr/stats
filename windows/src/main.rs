#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use std::{
    collections::VecDeque,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};
use sysinfo::{Disks, Networks, ProcessesToUpdate, System};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};

fn bytes(value: u64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut number = value as f64;
    let mut unit = 0;
    while number >= 1024.0 && unit < units.len() - 1 {
        number /= 1024.0;
        unit += 1;
    }
    format!("{number:.1} {}", units[unit])
}

struct Monitor {
    system: System,
    disks: Disks,
    networks: Networks,
    last: Instant,
    cpu_history: VecDeque<f32>,
    memory_history: VecDeque<f32>,
    download: u64,
    upload: u64,
    battery: String,
}

impl Monitor {
    fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_cpu_usage();
        Self {
            system,
            disks: Disks::new_with_refreshed_list(),
            networks: Networks::new_with_refreshed_list(),
            last: Instant::now(),
            cpu_history: VecDeque::new(),
            memory_history: VecDeque::new(),
            download: 0,
            upload: 0,
            battery: "Reading…".into(),
        }
    }
    fn refresh(&mut self) {
        let elapsed = self.last.elapsed().as_secs_f64();
        if elapsed < 1.0 {
            return;
        }
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.system.refresh_processes(ProcessesToUpdate::All, true);
        self.disks.refresh(true);
        self.networks.refresh(true);
        self.download =
            (self.networks.values().map(|n| n.received()).sum::<u64>() as f64 / elapsed) as u64;
        self.upload =
            (self.networks.values().map(|n| n.transmitted()).sum::<u64>() as f64 / elapsed) as u64;
        self.cpu_history.push_back(self.system.global_cpu_usage());
        self.memory_history.push_back(self.memory_percent());
        for history in [&mut self.cpu_history, &mut self.memory_history] {
            if history.len() > 120 {
                history.pop_front();
            }
        }
        self.battery = match battery::Manager::new().and_then(|m| m.batteries()) {
            Ok(batteries) => {
                let values: Vec<String> = batteries
                    .filter_map(Result::ok)
                    .map(|b| {
                        format!(
                            "{:.0}% · {:?}",
                            b.state_of_charge().value * 100.0,
                            b.state()
                        )
                    })
                    .collect();
                if values.is_empty() {
                    "No battery detected".into()
                } else {
                    values.join(" / ")
                }
            }
            Err(_) => "Battery information unavailable".into(),
        };
        self.last = Instant::now();
    }
    fn memory_percent(&self) -> f32 {
        if self.system.total_memory() == 0 {
            0.0
        } else {
            self.system.used_memory() as f32 / self.system.total_memory() as f32 * 100.0
        }
    }
}

struct Stats {
    monitor: Monitor,
    tray: TrayIcon,
    show: MenuItem,
    quit: MenuItem,
    filter: String,
    hidden: bool,
    events: Receiver<MenuEvent>,
}

impl Stats {
    fn new(
        cc: &eframe::CreationContext<'_>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let menu = Menu::new();
        let show = MenuItem::new("Show Stats", true, None);
        let quit = MenuItem::new("Quit", true, None);
        menu.append_items(&[&show, &quit])?;
        let mut rgba = vec![0u8; 32 * 32 * 4];
        for y in 0..32 {
            for x in 0..32 {
                let i = (y * 32 + x) * 4;
                let bar = (5..10).contains(&x) && y >= 18
                    || (13..18).contains(&x) && y >= 10
                    || (21..26).contains(&x) && y >= 4;
                rgba[i..i + 4].copy_from_slice(if bar {
                    &[80, 190, 255, 255]
                } else {
                    &[24, 28, 36, 255]
                });
            }
        }
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Stats for Windows")
            .with_icon(Icon::from_rgba(rgba, 32, 32)?)
            .build()?;
        let (sender, events) = mpsc::channel();
        let context = cc.egui_ctx.clone();
        MenuEvent::set_event_handler(Some(move |event| {
            let _ = sender.send(event);
            context.request_repaint();
        }));
        Ok(Self {
            monitor: Monitor::new(),
            tray,
            show,
            quit,
            filter: String::new(),
            hidden: false,
            events,
        })
    }
}

fn history(ui: &mut egui::Ui, values: &VecDeque<f32>, color: egui::Color32) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 90.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 6.0, egui::Color32::from_rgb(24, 28, 36));
    if values.len() > 1 {
        let points: Vec<_> = values
            .iter()
            .enumerate()
            .map(|(i, value)| {
                egui::pos2(
                    rect.left() + i as f32 / 119.0 * rect.width(),
                    rect.bottom() - value.clamp(0.0, 100.0) / 100.0 * rect.height(),
                )
            })
            .collect();
        ui.painter()
            .add(egui::Shape::line(points, egui::Stroke::new(2.0_f32, color)));
    }
}

impl eframe::App for Stats {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        while let Ok(event) = self.events.try_recv() {
            if event.id == self.quit.id() {
                std::process::exit(0);
            }
            if event.id == self.show.id() {
                self.hidden = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.hidden = true;
        }
        self.monitor.refresh();
        let m = &self.monitor;
        let _ = self.tray.set_tooltip(Some(format!(
            "CPU {:.0}% | RAM {:.0}% | ↓ {}/s | ↑ {}/s",
            m.system.global_cpu_usage(),
            m.memory_percent(),
            bytes(m.download),
            bytes(m.upload)
        )));
        ctx.request_repaint_after(Duration::from_secs(1));
        if self.hidden {
            return;
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Stats / Windows");
            ui.label(format!("{} · {} · Uptime {} h", System::host_name().unwrap_or_default(),
                System::long_os_version().unwrap_or_default(), System::uptime() / 3600));
            ui.label("Refresh: 1 second · Closing this window keeps Stats in the system tray.");
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.columns(2, |columns| {
                    columns[0].heading(format!("CPU  {:.1}%", m.system.global_cpu_usage()));
                    history(&mut columns[0], &m.cpu_history, egui::Color32::from_rgb(80, 190, 255));
                    columns[1].heading(format!("Memory  {:.1}%", m.memory_percent()));
                    history(&mut columns[1], &m.memory_history, egui::Color32::from_rgb(165, 135, 255));
                    columns[1].label(format!("{} / {}", bytes(m.system.used_memory()), bytes(m.system.total_memory())));
                });
                ui.collapsing("CPU cores", |ui| {
                    for (i, cpu) in m.system.cpus().iter().enumerate() {
                        ui.add(egui::ProgressBar::new(cpu.cpu_usage() / 100.0).text(format!("Core {} · {:.1}%", i + 1, cpu.cpu_usage())));
                    }
                });
                ui.separator();
                ui.heading("Network");
                ui.label(format!("Download {}/s    Upload {}/s", bytes(m.download), bytes(m.upload)));
                for (name, network) in &m.networks {
                    ui.label(format!("{name}: received {} · sent {}", bytes(network.total_received()), bytes(network.total_transmitted())));
                }
                ui.separator();
                ui.heading("Disks");
                for disk in &m.disks {
                    let total = disk.total_space();
                    let used = total.saturating_sub(disk.available_space());
                    let fraction = if total == 0 { 0.0 } else { used as f32 / total as f32 };
                    ui.add(egui::ProgressBar::new(fraction).text(format!("{} · {} / {}", disk.mount_point().display(), bytes(used), bytes(total))));
                }
                ui.separator();
                ui.label(format!("Battery: {}", m.battery));
                ui.separator();
                ui.heading("Processes · highest CPU first");
                ui.text_edit_singleline(&mut self.filter);
                let filter = self.filter.to_lowercase();
                let mut processes: Vec<_> = m.system.processes().iter().filter(|(_, p)| p.name().to_string_lossy().to_lowercase().contains(&filter)).collect();
                processes.sort_by(|a, b| b.1.cpu_usage().total_cmp(&a.1.cpu_usage()));
                egui::Grid::new("processes").striped(true).min_col_width(100.0).show(ui, |ui| {
                    ui.strong("PID"); ui.strong("Name"); ui.strong("CPU¹"); ui.strong("Memory"); ui.end_row();
                    for (pid, process) in processes.into_iter().take(30) {
                        ui.label(pid.to_string()); ui.label(process.name().to_string_lossy());
                        ui.label(format!("{:.1}%", process.cpu_usage())); ui.label(bytes(process.memory())); ui.end_row();
                    }
                });
                ui.small("¹ Process CPU can exceed 100% when multiple cores are used.");
                ui.separator();
                ui.small("Stats for Windows 0.1.0 · MIT · GPU, hardware sensors, fan control, Bluetooth and remote monitoring are not available in this version.");
            });
        });
    }
}

fn main() -> eframe::Result {
    if std::env::args().any(|arg| arg == "--self-test") {
        let mut monitor = Monitor::new();
        std::thread::sleep(Duration::from_millis(1100));
        monitor.refresh();
        assert!(!monitor.system.cpus().is_empty(), "No CPU data");
        assert!(monitor.system.total_memory() > 0, "No memory data");
        assert_eq!(monitor.cpu_history.len(), 1);
        assert!(monitor.system.global_cpu_usage().is_finite());
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([760.0, 820.0])
            .with_min_inner_size([520.0, 400.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Stats for Windows",
        options,
        Box::new(|cc| Ok(Box::new(Stats::new(cc)?))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formats_binary_units() {
        assert_eq!(bytes(0), "0.0 B");
        assert_eq!(bytes(1024), "1.0 KiB");
        assert_eq!(bytes(1_073_741_824), "1.0 GiB");
    }
    #[test]
    fn real_system_sample() {
        let mut m = Monitor::new();
        std::thread::sleep(Duration::from_millis(1100));
        m.refresh();
        assert!(m.system.total_memory() > 0);
        assert!((0.0..=100.0).contains(&m.memory_percent()));
        assert_eq!(m.cpu_history.len(), 1);
    }
}
