use cosmic::iced::mouse;
use cosmic::iced::widget::canvas::{self, Action, Event, Geometry, Path, Stroke};
use cosmic::iced::{Color, Point, Rectangle, Size};
use translator_core::Roi;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorMessage {
    Selected {
        roi: Roi,
        surface_width: u32,
        surface_height: u32,
    },
    Cancelled,
}

#[derive(Debug, Default, Clone)]
pub struct SelectorState {
    pub start: Option<Point>,
    pub current: Option<Point>,
}

#[derive(Debug, Default, Clone)]
pub struct SelectorProgram {
    pub preview_image: Option<cosmic::iced::core::image::Handle>,
    pub image_size: Option<(u32, u32)>,
}

impl SelectorProgram {
    pub fn new(preview_path: Option<&std::path::Path>) -> Self {
        if let Some(path) = preview_path {
            if path.exists() {
                if let Ok(reader) = image::ImageReader::open(path) {
                    if let Ok(img) = reader.decode() {
                        let size = (img.width(), img.height());
                        let handle = cosmic::iced::core::image::Handle::from_path(path.to_path_buf());
                        return Self {
                            preview_image: Some(handle),
                            image_size: Some(size),
                        };
                    }
                }
            }
        }
        Self::default()
    }
}

impl canvas::Program<SelectorMessage, cosmic::Theme, cosmic::Renderer> for SelectorProgram {
    type State = SelectorState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<SelectorMessage>> {
        let cursor_pos = cursor.position_in(bounds)?;

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                state.start = Some(cursor_pos);
                state.current = Some(cursor_pos);
                Some(Action::request_redraw().and_capture())
            }

            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.start.is_some() => {
                state.current = Some(cursor_pos);
                Some(Action::request_redraw())
            }

            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let (Some(start), Some(curr)) = (state.start.take(), state.current.take()) {
                    let min_x = start.x.min(curr.x);
                    let min_y = start.y.min(curr.y);
                    let width = (curr.x - start.x).abs();
                    let height = (curr.y - start.y).abs();

                    if width >= 10.0 && height >= 10.0 {
                        let (roi_x, roi_y, roi_w, roi_h) = if let Some((img_w, img_h)) = self.image_size {
                            let scale_x = img_w as f32 / bounds.width.max(1.0);
                            let scale_y = img_h as f32 / bounds.height.max(1.0);
                            (
                                ((min_x * scale_x).round() as u32).min(img_w),
                                ((min_y * scale_y).round() as u32).min(img_h),
                                ((width * scale_x).round() as u32).min(img_w),
                                ((height * scale_y).round() as u32).min(img_h),
                            )
                        } else {
                            (
                                min_x.max(0.0) as u32,
                                min_y.max(0.0) as u32,
                                width as u32,
                                height as u32,
                            )
                        };

                        let roi = Roi::new(roi_x, roi_y, roi_w, roi_h);
                        Some(Action::publish(SelectorMessage::Selected {
                            roi,
                            surface_width: self.image_size.map(|(w, _)| w).unwrap_or(bounds.width.max(1.0) as u32),
                            surface_height: self.image_size.map(|(_, h)| h).unwrap_or(bounds.height.max(1.0) as u32),
                        }))
                    } else {
                        Some(Action::request_redraw())
                    }
                } else {
                    None
                }
            }

            Event::Keyboard(cosmic::iced::keyboard::Event::KeyPressed {
                key: cosmic::iced::keyboard::Key::Named(cosmic::iced::keyboard::key::Named::Escape),
                ..
            }) => Some(Action::publish(SelectorMessage::Cancelled)),

            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &cosmic::Renderer,
        _theme: &cosmic::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        if let Some(ref handle) = self.preview_image {
            frame.draw_image(bounds, handle);
            frame.fill_rectangle(
                Point::ORIGIN,
                bounds.size(),
                Color::from_rgba(0.0, 0.0, 0.0, 0.35),
            );
        } else {
            frame.fill_rectangle(
                Point::ORIGIN,
                bounds.size(),
                Color::from_rgba(0.0, 0.0, 0.0, 0.45),
            );
        }

        // 2. If user is dragging a selection rectangle
        if let (Some(start), Some(curr)) = (state.start, state.current) {
            let x = start.x.min(curr.x);
            let y = start.y.min(curr.y);
            let w = (curr.x - start.x).abs();
            let h = (curr.y - start.y).abs();

            // Inner translucent highlight
            frame.fill_rectangle(
                Point::new(x, y),
                Size::new(w, h),
                Color::from_rgba(0.28, 0.58, 0.94, 0.20),
            );

            // Crisp accent border
            frame.stroke(
                &Path::rectangle(Point::new(x, y), Size::new(w, h)),
                Stroke::default()
                    .with_width(2.0)
                    .with_color(Color::from_rgb(0.28, 0.58, 0.94)),
            );
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.is_over(bounds) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}
