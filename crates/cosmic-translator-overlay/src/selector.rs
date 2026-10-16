use cosmic::iced::mouse;
use cosmic::iced::widget::canvas::{self, Action, Event, Geometry, Path, Stroke};
use cosmic::iced::{Color, Point, Rectangle, Size};
use translator_core::Roi;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorMessage {
    Selected(Roi),
    Cancelled,
}

#[derive(Debug, Default, Clone)]
pub struct SelectorState {
    pub start: Option<Point>,
    pub current: Option<Point>,
}

#[derive(Default)]
pub struct SelectorProgram;

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
                        let roi = Roi::new(
                            min_x.max(0.0) as u32,
                            min_y.max(0.0) as u32,
                            width as u32,
                            height as u32,
                        );
                        Some(Action::publish(SelectorMessage::Selected(roi)))
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

        // 1. Semi-transparent dark shroud covering whole screen
        frame.fill_rectangle(
            Point::ORIGIN,
            bounds.size(),
            Color::from_rgba(0.0, 0.0, 0.0, 0.45),
        );

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
