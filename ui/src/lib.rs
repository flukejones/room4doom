pub mod finale;
pub mod hud_messages;
pub mod intermission;
pub mod menu;
pub mod statusbar;

pub use finale::Finale;
pub use hud_messages::Messages;
pub use intermission::Intermission;
pub use menu::GameMenu;
pub use statusbar::Statusbar;

#[cfg(test)]
pub(crate) mod tests {
    use game_config::GameMode;
    use gamestate_traits::SubsystemTrait as _;
    use render_common::{BufferSize, DrawBuffer, UiLayout};
    use test_utils::doom1_wad_path;
    use wad::WadData;

    use super::{Finale, Intermission, Messages, Statusbar};

    pub(crate) struct Surface {
        pub(crate) pixels: Vec<u32>,
        size: BufferSize,
        layout: UiLayout,
    }

    impl Surface {
        pub(crate) fn new(w: u32, h: u32, display: (u32, u32)) -> Self {
            Self {
                pixels: vec![0xFF01_0203; (w * h) as usize],
                size: BufferSize::new(w as usize, h as usize),
                layout: UiLayout::from_sizes((w, h), display),
            }
        }

        pub(crate) fn capture(&self, name: &str) {
            assert!(
                self.pixels.iter().any(|&p| p != 0xFF01_0203),
                "{name} drew pixels"
            );
            if let Some(directory) = std::env::var_os("ROOM4DOOM_UI_CAPTURE") {
                let directory = std::path::PathBuf::from(directory);
                std::fs::create_dir_all(&directory).expect("capture directory");
                let mut ppm =
                    format!("P6\n{} {}\n255\n", self.size.width(), self.size.height()).into_bytes();
                for &pixel in &self.pixels {
                    let [_, red, green, blue] = pixel.to_be_bytes();
                    ppm.extend_from_slice(&[red, green, blue]);
                }
                std::fs::write(directory.join(format!("{name}.ppm")), ppm).expect("UI capture");
            }
        }
    }

    impl DrawBuffer for Surface {
        type Pixel = u32;
        fn size(&self) -> &BufferSize {
            &self.size
        }
        fn ui_layout(&self) -> UiLayout {
            self.layout
        }
        fn pitch(&self) -> usize {
            self.size.width_usize()
        }
        fn get_buf_index(&self, x: usize, y: usize) -> usize {
            y * self.pitch() + x
        }
        fn set_pixel(&mut self, x: usize, y: usize, color: u32) {
            assert!(x < self.size.width_usize() && y < self.size.height_usize());
            let index = self.get_buf_index(x, y);
            self.pixels[index] = color;
        }
        fn buf_mut(&mut self) -> &mut [u32] {
            &mut self.pixels
        }
    }

    #[test]
    fn overlays_share_classic_native_and_fractional_draw_targets() {
        let wad = WadData::new(&doom1_wad_path());
        let mut hud = Statusbar::new(GameMode::Shareware, &wad);
        let mut messages = Messages::new(&wad);
        messages.add_line("NATIVE HUD MESSAGE".to_owned());
        let mut intermission = Intermission::new(GameMode::Shareware, &wad, &None);
        let mut finale = Finale::new(&wad);
        for (w, h, display) in [
            (320, 200, (640, 480)),
            (960, 720, (960, 720)),
            (721, 541, (721, 541)),
            (480, 640, (480, 640)),
        ] {
            let mut surface = Surface::new(w, h, display);
            hud.draw(&mut surface);
            messages.draw(&mut surface);
            surface.capture(&format!("hud-{w}x{h}"));
            let mut surface = Surface::new(w, h, display);
            intermission.draw(&mut surface);
            surface.capture(&format!("intermission-{w}x{h}"));
            let mut surface = Surface::new(w, h, display);
            finale.draw(&mut surface);
            surface.capture(&format!("finale-{w}x{h}"));
        }
    }
}
