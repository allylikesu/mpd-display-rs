use std::path::Path;
use std::cmp::{min, max};
use std::io::Cursor;
use std::process::Command;
use speedy2d::Window;
use speedy2d::color::Color;
use speedy2d::window::{WindowHelper, WindowFullscreenMode, WindowStartupInfo, VirtualKeyCode, KeyScancode, MouseButton};
use speedy2d::Graphics2D;
use speedy2d::dimen::{UVec2, Vec2};
use speedy2d::shape::{Rectangle, RoundedRectangle};
use speedy2d::font::{Font, FormattedTextBlock, TextLayout, TextOptions};
use speedy2d::image::{ImageHandle, ImageFileFormat, ImageSmoothingMode};
use mpd::Client;
use mpd::status::Status;
use mpd::song::{Song, Id};
use log::{info, trace, warn};

struct MyWindowHandler {
    linewidth: f32,
    width: u32,
    height: u32,
    fullscreen: bool,
    bar_hover: bool,
    cursor_visible: bool,
    show_debug_window: bool,
    startup: bool,


    mpd_client: Client,
    mpd_status: Status,
    current_song: Option<Song>,
    current_song_id: u32,
    queue_len: u32,
    next_song: Option<Song>,

    font_light: Font,
    font_bold: Font,

    text_playingfromqueue: Option<FormattedTextBlock>,
    text_queue: Option<FormattedTextBlock>,
    text_title: Option<FormattedTextBlock>,
    text_artist: Option<FormattedTextBlock>,
    text_upnext: Option<FormattedTextBlock>,
    text_next_song: Option<FormattedTextBlock>,

    text_color_background: Color,
    text_color_foreground: Color,
    text_color_midground: Color,
    color_background_image_tint: Color,
    color_background: Color,
    color_accent: Color,

    image_background: Option<ImageHandle>,
    image_watermark: Option<ImageHandle>,
    image_album: Option<ImageHandle>,
    backup_album_image: Option<ImageHandle>,
}
impl egui_speedy2d::WindowHandler for MyWindowHandler {

    // Init params on startup
    //INIT
    fn on_start(
        &mut self,
        helper: &mut WindowHelper,
        info: WindowStartupInfo,
        _egui_ctx: &egui::Context,
    ) {
        let geom = info.viewport_size_pixels();
        self.width = geom.x;
        self.height = geom.y;
        self.update_fullscreen(helper);
        helper.set_cursor_visible(self.cursor_visible);
        self.update_queue_len_text();
        self.update_text();
    }

    fn on_draw(
        &mut self, 
        helper: &mut WindowHelper, 
        graphics: &mut Graphics2D,
        egui_ctx: &egui::Context,
    ) {
        if self.startup {
            self.startup = false;
            self.init_images(graphics);
        }
        graphics.clear_screen(self.color_background);

        self.update_mpd(graphics);
        if self.queue_len != self.mpd_status.queue_len {
            self.queue_len = self.mpd_status.queue_len;
            self.update_queue_len_text();
        }
        //DRAW
        //
        // draw BACKGROUND
        match &self.image_background {
            None => {},
            Some(handle) => {
                let img_dims = handle.size();
                let mut x_offset = 0.0;
                let mut y_offset = 0.0;
                let mut scale = self.width as f32 / img_dims.x as f32;
                if (img_dims.y as f32 * scale) < self.height as f32 {
                    scale = self.height as f32 / img_dims.y as f32;
                    x_offset = (self.width as f32 / -2.0) + (img_dims.x as f32 * scale / 2.0);
                }
                else {
                    y_offset = (self.height as f32 / -2.0) + (img_dims.y as f32 * scale / 2.0);
                }
                let rect = get_scaled_image_rect(handle, scale, (-x_offset, -y_offset));
                graphics.draw_rectangle_image_tinted(rect, self.color_background_image_tint, handle);
            },
        }
        // draw WATERMARK
        let mut watermark_image_size = match &self.image_watermark {
            None => UVec2::new(1,1),
            Some(handle) => handle.size().clone(),
        };
        let watermark_x_offset = self.width as f32 / 20.0;
        let watermark_y_offset = self.height as f32 / 20.0;
        let watermark_resize_scale = (self.height as f32/9.0)/watermark_image_size.y as f32;
        let watermark_rect = match &self.image_watermark {
            None => Rectangle::from_tuples((watermark_x_offset, watermark_y_offset), (watermark_x_offset, watermark_y_offset)),
            Some(handle) => get_scaled_image_rect(handle, watermark_resize_scale, (watermark_x_offset, watermark_y_offset)),
        };
        watermark_image_size = UVec2::new(watermark_rect.width() as u32, watermark_rect.height() as u32);
        match &self.image_watermark {
            None => {},
            Some(handle) => {
                graphics.draw_rectangle_image(watermark_rect, handle);
            }
        };
        // draw PLAYINGFROM
        let mpd_text_x_offset = watermark_x_offset as f32 + watermark_image_size.x as f32 + watermark_image_size.y as f32 / 4.0;
        let mpd_text_size = match &self.text_playingfromqueue {
            Some(text) => text.size(),
            None => Vec2::new(1.0,1.0),
        };
        let mpd_text_y_offset = watermark_y_offset as f32 + mpd_text_size.y;
        match &self.text_playingfromqueue {
            None => {}
            Some(text) => {
                graphics.draw_text((mpd_text_x_offset, mpd_text_y_offset), self.text_color_background, &text);
            }
        };
        // draw QUEUE
        let queue_text_size = match &self.text_queue {
            Some(text) => text.size(),
            None => Vec2::new(1.0, 1.0),
        };
        let queue_y_offset = mpd_text_y_offset + queue_text_size.y;
        match &self.text_queue {
            None => {},
            Some(text) => {
                graphics.draw_text((mpd_text_x_offset, queue_y_offset), self.text_color_background, &text);
            }
        };
        // draw ALBUMART
        let album_image_size = match &self.image_album {
            None => UVec2::new(1,1),
            Some(handle) => handle.size().clone(),
        };
        let album_resize_value = min(self.height, self.width) as f32 / 3.0;
        let album_y_offset = self.height as f32 / 6.0 * 5.0 - album_resize_value;
        let album_x_offset = self.width as f32 / 16.0;
        let album_rect = Rectangle::from_tuples((album_x_offset, album_y_offset), (album_x_offset + album_resize_value, album_y_offset + album_resize_value));
        match &self.image_album {
            None => {},
            Some(handle) => {
                graphics.draw_rectangle_image(album_rect, handle);
            }
        };
        // draw TITLE
        let title_x_offset = album_x_offset + album_resize_value * 1.1;
        let title_y_offset = album_y_offset + album_resize_value * 0.4;
        let mut title_height = 1.0;
        match &self.text_title {
            None => {},
            Some(text) => {
                graphics.draw_text((title_x_offset, title_y_offset), self.text_color_foreground, &text);
                title_height = text.height();
            },
        };
        // draw ARTIST
        let artist_y_offset = title_y_offset + title_height + album_resize_value * 0.05;
        match &self.text_artist {
            None => {},
            Some(text) => {
                graphics.draw_text((title_x_offset, artist_y_offset), self.text_color_midground, &text);
            }
        };
        // draw PROGRESSBAR:bar
        let bar_background = Color::from_rgba(self.text_color_background.r(), self.text_color_background.g(), self.text_color_background.b(), 0.5);
        let bar_width = self.width as f32 * 0.85;
        let bar_offset_x = (self.width as f32 - bar_width) * 0.5;
        let bar_height = self.height as f32 * 0.006;
        let bar_offset_y = self.height as f32 * 0.9;
        let mut bar_progress_color = self.text_color_foreground;
        if self.bar_hover {
            bar_progress_color = self.color_accent;
        }
        let bar_back_rect = RoundedRectangle::from_tuples((bar_offset_x, bar_offset_y), (bar_offset_x + bar_width, bar_offset_y + bar_height),  bar_height / 2.1);
        graphics.draw_rounded_rectangle(bar_back_rect, bar_background);
        let song_elapsed = match self.mpd_status.elapsed {
            Some(duration) => {
                duration.as_secs_f32()
            },
            None => {
                0.0
            }
        };
        let mut song_duration = match self.mpd_status.duration {
            Some(duration) => {
                duration.as_secs_f32()
            },
            None => {
                1.0
            }
        };
        if song_duration == 0.0 { song_duration = 1.0; }
        let song_percentage = song_elapsed / song_duration;
        let bar_progress_length = bar_width * song_percentage;
        let bar_progress_rect = RoundedRectangle::from_tuples((bar_offset_x, bar_offset_y), (bar_offset_x + bar_progress_length, bar_offset_y + bar_height), bar_height / 2.1);
        graphics.draw_rounded_rectangle(bar_progress_rect, bar_progress_color);
        // draw PROGRESSBAR:elapsed
        let (secs_elapsed, secs_duration) = match self.mpd_status.time {
            Some((elapsed, duration)) => {
                (elapsed.as_secs(), duration.as_secs())
            },
            None => {
                (0, 1)
            }
        };
        let mut elapsed_secs_str = (secs_elapsed % 60).to_string();
        if secs_elapsed % 60 < 10 {
            elapsed_secs_str = format!("0{}", secs_elapsed % 60);
        }
        let mut duration_secs_str = (secs_duration % 60).to_string();
        if secs_duration % 60 < 10 {
            duration_secs_str = format!("0{}", secs_duration % 60);
        }
        let elapsed_str = format!("{}:{}", secs_elapsed / 60, elapsed_secs_str);
        let duration_str = format!("{}:{}", secs_duration / 60, duration_secs_str);
        let bar_fontsize = bar_height * 3.0;
        let text_elapsed = self.font_light.layout_text(&elapsed_str, bar_fontsize, TextOptions::new());
        let elapsed_x_offset = bar_offset_x - text_elapsed.width() - bar_height * 1.5;
        let bar_text_y_offset = bar_offset_y - text_elapsed.height() / 3.1;
        graphics.draw_text((elapsed_x_offset, bar_text_y_offset), self.text_color_foreground, &text_elapsed);
        // draw PROGRESSBAR:duration
        let text_duration = self.font_light.layout_text(&duration_str, bar_fontsize, TextOptions::new());
        let duration_x_offset = bar_offset_x + bar_width + bar_height * 1.5;
        graphics.draw_text((duration_x_offset, bar_text_y_offset), self.text_color_foreground, &text_duration);
        if self.bar_hover {
            let circle_x = bar_offset_x + bar_progress_length;
            let circle_y = bar_offset_y + bar_height * 0.5;
            graphics.draw_circle((circle_x, circle_y), bar_height, self.text_color_foreground);
        }
        // draw UPNEXT
        if song_percentage >= 0.9 {
            let next_offset_y = self.height as f32 / 20.0;
            //let next_offset_x = (self.width as i32 - max(self.text_upnext.as_ref().unwrap().width() as i32, self.text_next_song.as_ref().unwrap().width() as i32) - self.width as i32) as f32 / 30.0;
            let next_offset_x = self.width as f32 - max(self.text_upnext.as_ref().unwrap().width() as i32, self.text_next_song.as_ref().unwrap().width() as i32) as f32 - self.width as f32 / 30.0;
            let barratio = song_percentage * 10.0 - 9.0;
            let baroffset = (self.width as f32 - next_offset_x)*barratio;
            graphics.draw_text((next_offset_x, next_offset_y), self.text_color_foreground, self.text_upnext.as_ref().unwrap());
            graphics.draw_text((next_offset_x, next_offset_y + self.text_upnext.as_ref().unwrap().height()), self.text_color_foreground, self.text_next_song.as_ref().unwrap());
            let bar_fontsize = self.height as f32 / 30.0;
            let barrect_y_offset = next_offset_y + self.text_upnext.as_ref().unwrap().height() + self.text_next_song.as_ref().unwrap().height() + bar_fontsize / 2.0;
            let barrect = RoundedRectangle::from_tuples((next_offset_x + baroffset, barrect_y_offset), (self.width as f32 + 5.0, barrect_y_offset + bar_height), bar_height / 2.1);
            graphics.draw_rounded_rectangle(barrect, self.text_color_foreground);
        }

        /*if self.linewidth >= self.width as f32 {
            self.linewidth = 0.0;
        } else {
            self.linewidth += 2.0;
        }
        graphics.draw_line((0.0,0.0), (self.linewidth, 0.0), 10.0, Color::RED);*/
        //GUI
        // egui window
        if self.show_debug_window {
            egui::Window::new("Debug").show(&egui_ctx, |ui| {
                ui.heading("Window");
                ui.label("(Note: will not change window geometry)");
                ui.add(egui::Slider::new(&mut self.width, 0..=1920).text("Width"));
                ui.add(egui::Slider::new(&mut self.height, 0..=1080).text("Height"));
                if ui.checkbox(&mut self.fullscreen, "Fullscreen").clicked() {
                    self.update_fullscreen(helper);
                };
            });
        }

        helper.request_redraw();
    }

    // Handle key presses
    //KEY
    fn on_key_down(
        &mut self,
        helper: &mut WindowHelper,
        keycode: Option<VirtualKeyCode>,
        _scancode: KeyScancode,
        _egui_ctx: &egui::Context,
    ) {
        match keycode {
            // F: Toggle fullscreen
            Some(VirtualKeyCode::F) => {
                self.fullscreen = !self.fullscreen;
                self.update_fullscreen(helper);
            },

            // C: Toggle Cursor Visibility
            Some(VirtualKeyCode::C) => {
                self.cursor_visible = !self.cursor_visible;
                helper.set_cursor_visible(self.cursor_visible);
            },

            // D: Toggle debug panel visibility
            Some(VirtualKeyCode::D) => {
                self.show_debug_window = !self.show_debug_window;
            },

            Some(VirtualKeyCode::Space) => {
                let _ = self.mpd_client.toggle_pause();
            },

            Some(VirtualKeyCode::Escape) => {
                helper.terminate_loop();
            },
            _ => {},
        }
    }

    fn on_mouse_button_down(
        &mut self,
        _helper: &mut WindowHelper,
        button: MouseButton,
        _egui_ctx: &egui::Context,
    ) {
        match button {
            MouseButton::Left => {
                let _ = self.mpd_client.toggle_pause();
            },
            _ => {
            },
        }
    }

    fn on_resize(
        &mut self,
        _helper: &mut WindowHelper,
        size_pixels: UVec2,
        _egui_ctx: &egui::Context,
    ) {
        self.width = size_pixels.x;
        self.height = size_pixels.y;
        self.update_text();
    }

    fn on_fullscreen_status_changed(
        &mut self,
        _helper: &mut WindowHelper,
        new_fullscreen: bool,
        _egui_ctx: &egui::Context,
    ) {
        self.fullscreen = new_fullscreen;
    }

    fn on_mouse_move(
        &mut self,
        _helper: &mut WindowHelper,
        position: Vec2,
        _egui_ctx: &egui::Context,
    ) {
        let bar_offset_y = self.height as f32 * 0.9;
        let bar_height = self.height as f32 * 0.006;
        let bar_middle = bar_offset_y - bar_height * 0.5;
        let margins = self.height as f32 * 0.02;
        let bar_low_bound = bar_middle + margins;
        let bar_high_bound = bar_middle - margins;
        if position.y < bar_low_bound && position.y > bar_high_bound {
            self.bar_hover = true;
        } else { self.bar_hover = false; }
    }
}
impl MyWindowHandler {
    fn update_fullscreen(&mut self, helper: &mut WindowHelper) {
        let fullscreen_mode: WindowFullscreenMode = match self.fullscreen {
            true => WindowFullscreenMode::FullscreenBorderless,
            false => WindowFullscreenMode::Windowed,
        };
        helper.set_fullscreen_mode(fullscreen_mode);
    }

    // Runs every time the window is resized
    fn update_text(&mut self) {
        //text_payingfromqueue
        // PLAYING FROM MPD QUEUE size: height / 27
        // "X tracks left" size: height / 36
        let (title, artist) = match &self.current_song {
            Some(song) => (song.title.clone().get_or_insert(song.file.clone()).to_owned(), song.artist.clone().get_or_insert("".to_owned()).to_owned()),
            None => ("Nothing playing".to_owned(), "".to_owned()),
        };
        self.text_playingfromqueue = Some(self.font_light.layout_text("PLAYING FROM MPD QUEUE", self.height as f32 / 42.0, TextOptions::new()));
        self.update_queue_len_text();
        let mut title_font_size = min(self.height, self.width) as f32 / 9.0;
        let title_x_offset = self.width as f32 / 16.0 + min(self.height, self.width) as f32 / 3.0 * 1.1;
        let title_available_pixels = (self.width as f32 - title_x_offset) * 0.97;
        self.text_title = Some(self.font_bold.layout_text(&title, title_font_size, TextOptions::new()));
        while self.text_title.as_ref().unwrap().width() >= title_available_pixels {
            title_font_size = title_font_size * 0.95;
            self.text_title = Some(self.font_bold.layout_text(&title, title_font_size, TextOptions::new()));
        }
        title_font_size = min(self.height, self.width) as f32 / 9.0;
        self.text_artist = Some(self.font_bold.layout_text(&artist, title_font_size / 2.0, TextOptions::new()));
        while self.text_artist.as_ref().unwrap().width() >= title_available_pixels {
            title_font_size = title_font_size * 0.95;
            self.text_artist = Some(self.font_bold.layout_text(&artist, title_font_size / 2.0, TextOptions::new()));
        }

        let (next_title, next_artist) = match &self.next_song {
            Some(song) => (song.title.clone().get_or_insert(song.file.clone()).to_owned(), song.artist.clone().get_or_insert("".to_owned()).to_owned()),
            None => ("Nothing".to_owned(), "".to_owned()),
        };
        let upnext_fontsize = self.height as f32 / 30.0;
        self.text_upnext = Some(self.font_bold.layout_text("Up next:", upnext_fontsize * 1.1, TextOptions::new()));
        let mut artist_str = next_artist.to_owned();
        if artist_str != "".to_owned() {
            artist_str = next_artist + " -";
        }
        self.text_next_song = Some(self.font_light.layout_text(&format!("{} {}", artist_str, next_title), upnext_fontsize, TextOptions::new()));
    }

    fn init_images(&mut self, ctx: &mut Graphics2D) {
        /*self.image_background = match ctx.create_image_from_file_path(None, ImageSmoothingMode::Linear, Path::new("./artists/temp.png")) {
            Ok(handle) => Some(handle),
            Err(e) => {
                println!("error initializing background image: {}", e);
                None
            },
        };*/
        self.image_watermark = match ctx.create_image_from_file_path(Some(ImageFileFormat::PNG), ImageSmoothingMode::Linear, Path::new("./assets/logo.png")) {
            Ok(handle) => Some(handle),
            Err(e) => {
                println!("error initializing watermark image: {}", e);
                None
            }
        };
        self.backup_album_image = match ctx.create_image_from_file_path(Some(ImageFileFormat::PNG), ImageSmoothingMode::Linear, Path::new("./assets/art_backup")) {
            Ok(handle) => Some(handle),
            Err(e) => {
                println!("error initializing default album image: {}", e);
                None
            },
        };
        self.update_images(ctx);
    }

    fn update_mpd(&mut self, ctx: &mut Graphics2D) {
        let old_song_id = self.current_song_id.clone();
        self.mpd_status = self.mpd_client.status().unwrap();
        (self.current_song, self.current_song_id) = match self.mpd_status.song {
            Some(queue_place) => {
                let song = match self.mpd_client.playlistid(queue_place.id) {
                    Ok(s) => s,
                    Err(e) => {println!("No song exists: {}", e); None},
                };
                if song == None { return; }
                (self.mpd_client.playlistid(queue_place.id).unwrap(), queue_place.id.0)
            },
            None => (None, u32::MAX),
        };
        self.next_song = match self.mpd_status.nextsong {
            Some(queue_place) => {
                self.mpd_client.playlistid(queue_place.id).unwrap()
            },
            None => None,
        };
        if self.current_song_id != old_song_id {
            self.update_text();
            self.update_images(ctx);
        }
    }

    fn update_queue_len_text(&mut self) {
        let contents = format!(
            "{} tracks left",
            self.mpd_status.queue_len
        );
        self.text_queue = Some(self.font_bold.layout_text(&contents, self.height as f32 / 31.0, TextOptions::new()));
    }

    fn update_images(&mut self, ctx: &mut Graphics2D) {
        match &self.current_song {
            Some(song) => {
                match &song.artist {
                    Some(artist) => {
                        let first_artist = artist
                            .split(", ").next().unwrap()
                            .split("/").next().unwrap()
                            .split(" & ").next().unwrap()
                            .split("; ").next().unwrap()
                            .to_lowercase();
                        self.image_background = match ctx.create_image_from_file_path(None, ImageSmoothingMode::Linear, Path::new(&format!("./artists/{}.jpg", first_artist))) {
                            Ok(handle) => Some(handle),
                            Err(e) => {
                                println!("error initializing background image: {}", e);
                                None
                            },
                        };
                    },
                    None => {
                        self.image_background = None;
                    },
                };
                self.image_album = match Command::new("mpc")
                    .arg("readpicture")
                    .arg(song.file.clone())
                    .output() {
                        Ok(output) => {
                            //output.stdout
                            match ctx.create_image_from_file_bytes(None, ImageSmoothingMode::Linear, Cursor::new(output.stdout)) {
                                Ok(handle) => Some(handle),
                                Err(e) => {
                                    println!("Error creating image from mpc output: {}", e);
                                    self.backup_album_image.clone()
                                }
                            }
                        },
                        Err(e) => {
                            println!("Error running mpc readpicture: {}", e);
                            self.backup_album_image.clone()
                        },
                };
                /*let albumart = match self.mpd_client.albumart(song) {
                    Ok(data) => {
                        match ctx.create_image_from_file_bytes(None, ImageSmoothingMode::Linear, Cursor::new(data)) {
                            Ok(handle) => {
                                handle
                            }
                            Err(e) => {
                                println!("Failed to init album art: {}", e);
                                ctx.create_image_from_file_path(Some(ImageFileFormat::PNG), ImageSmoothingMode::Linear, Path::new("./assets/art_backup")).unwrap()
                            }
                        }
                    },
                    Err(e) => {
                        println!("Failed to obtain album image data: {:?}", e);
                        ctx.create_image_from_file_path(Some(ImageFileFormat::PNG), ImageSmoothingMode::Linear, Path::new("./assets/art_backup")).unwrap()
                    }
                };
                self.image_album = Some(albumart);*/
            },
            None => {
                self.image_background = None;
            },
        }
    }

}

//MAIN
fn main() {
    simple_logger::SimpleLogger::new().init().unwrap();

    info!("Starting MPD connection and initializing client");
    let mut mpd_client = Client::connect("localhost:6600").unwrap();
    let mpd_status = mpd_client.status().unwrap();
    let (current_song, current_song_id) = match mpd_status.song {
        Some(queue_place) => {
            (mpd_client.playlistid(queue_place.id).unwrap(), queue_place.id.0)
        },
        None => (None, u32::MAX),
    };
    let next_song = match mpd_status.nextsong {
        Some(queue_place) => {
            mpd_client.playlistid(queue_place.id).unwrap()
        },
        None => None,
    };
    let queue_len = mpd_status.queue_len;
    println!("Status: {:?}", mpd_status);


    info!("Creating Window");
    let window = Window::new_fullscreen_borderless("MPD Display").unwrap();


    window.run_loop(egui_speedy2d::WindowWrapper::new(MyWindowHandler{
        linewidth: 0.0,
        width: 0,
        height: 0,
        fullscreen: true,
        bar_hover: false,
        cursor_visible: true,
        show_debug_window: false,
        startup: true,

        mpd_client,
        mpd_status,
        current_song,
        current_song_id,
        queue_len,
        next_song,

        font_light: Font::new(include_bytes!("../assets/font/CircularStd-Book.otf")).unwrap(),
        font_bold: Font::new(include_bytes!("../assets/font/CircularStd-Bold.otf")).unwrap(),

        text_playingfromqueue: None,
        text_queue: None,
        text_title: None,
        text_artist: None,
        text_upnext: None,
        text_next_song: None,

        text_color_background: Color::from_int_rgb(156,156,156),
        text_color_foreground: Color::from_int_rgb(255,255,255),
        text_color_midground: Color::from_int_rgb(195,195,195),
        color_background_image_tint: Color::from_int_rgba(75, 75, 75, 255),
        color_background: Color::from_int_rgb(50,50,50),
        color_accent: Color::from_int_rgb(29, 185, 84),

        image_background: None,
        image_watermark: None,
        image_album: None,
        backup_album_image: None,
    }));
}

fn get_scaled_image_rect(image: &ImageHandle, scale: f32, top_left: (f32, f32)) -> Rectangle<f32> {
    let (img_width, img_height) = (image.size().x, image.size().y);
    let new_width: f32 = (img_width as f32 * scale).ceil();
    let new_height: f32 = (img_height as f32 * scale).ceil();
    let bottom_right = (top_left.0 + new_width, top_left.1 + new_height);
    Rectangle::from_tuples(top_left, bottom_right)
}
