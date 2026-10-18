---
name: Iced Best Practices & Architecture
description: Comprehensive architecture, The Elm Architecture (TEA), layout sizing rules, task handling, and subscription patterns for iced and iced-based Rust GUI applications.
---

# Iced Best Practices & Architecture Guide

This guide defines the architectural standards, layout sizing rules, state management patterns, and asynchronous task execution models for applications built with `iced` (and `cosmic::iced`).

---

## 1. The Elm Architecture (TEA) in Rust

Iced structures GUI applications around five fundamental parts:

```
              +--------------------------+
              |          State           |
              |       (Application)      |
              +-------------+------------+
                            |
                     view(&self)
                            |
                            v
              +-------------+------------+
              |         Element          |
              |     (Widget Layout)      |
              +-------------+------------+
                            |
                   User Event / Action
                            |
                            v
              +-------------+------------+
              |         Message          |
              |      (enum Message)      |
              +-------------+------------+
                            |
               update(&mut self, Message)
                            |
                            v
+---------------------------+---------------------------+
|               Task / Action (Asynchronous)            |
+-------------------------------------------------------+
```

1. **State:** Pure data model representing application state.
2. **Messages (`Message`):** Strongly-typed enum representing every possible user action or external event.
3. **View:** Pure function `(&self) -> Element<'_, Message>`. Computes the widget tree from the current state.
4. **Update:** Pure state mutator `(&mut self, Message) -> Task<Message>`. Modifies state in response to messages and returns optional asynchronous tasks.
5. **Subscription:** Event listener `(&self) -> Subscription<Message>` yielding asynchronous streams (timers, sockets, global shortcuts, window events).

---

## 2. Layout & Sizing Rules

Iced uses a flexible box layout engine with four dimensional modes:

| Mode | Description | Example |
| :--- | :--- | :--- |
| `Length::Fill` | Expands greedily to consume all available parent space | `.width(Length::Fill)` |
| `Length::Shrink` | Shrinks tightly to fit inner children | `.height(Length::Shrink)` |
| `Length::Fixed(f32)` | Fixed size in logical pixels | `.width(Length::Fixed(400.0))` |
| `Length::FillPortion(u16)` | Relative ratio when sharing space with sibling widgets | `.width(Length::FillPortion(2))` |

### Common Layout Pitfalls

#### Pitfall 1: Unbounded Scrollable Panics
- **Problem:** Placing a `scrollable` with `Length::Shrink` inside an unbounded `column` causes infinite dimension calculation or panics.
- **Rule:** A `scrollable` widget MUST have an explicit boundary:
  ```rust
  // CORRECT
  widget::scrollable(content)
      .width(Length::Fill)
      .height(Length::Fill)
  ```

#### Pitfall 2: Text Overflow Offscreen
- **Problem:** `widget::text(&str)` does NOT wrap by default. Long strings overflow past window bounds.
- **Rule:** Always enable word wrapping for dynamic, user-generated, or localized strings:
  ```rust
  widget::text(&self.content)
      .wrapping(cosmic::iced::widget::text::Wrapping::Word)
  ```

#### Pitfall 3: Naked Window Content (Transparent Canvas Glitch)
- **Problem:** Returning an unwrapped `column` or `row` as a window root element produces a transparent background on Wayland, accumulating frame artifacts.
- **Rule:** Always wrap toplevel window views in an explicit background container:
  ```rust
  widget::container(content)
      .width(Length::Fill)
      .height(Length::Fill)
      .class(cosmic::theme::Container::WindowBackground)
  ```

---

## 3. Asynchronous Tasks (`Task`)

- Never block the UI thread (`std::thread::sleep` is forbidden inside `update` or `view`).
- Use `Task::perform` to run an async future and map its result to a `Message`:
  ```rust
  // Asynchronous network / disk operation
  Task::perform(
      async move {
          fetch_data().await
      },
      |result| Message::DataLoaded(result),
  )
  ```
- Use `Task::batch` to launch independent tasks concurrently:
  ```rust
  Task::batch([task_a, task_b])
  ```
- Return `Task::none()` when an event requires only immediate in-memory state mutation.

---

## 4. Subscriptions & External Streams

Subscriptions connect external asynchronous event loops to the Iced application lifecycle.

### Tokio Channel to Iced Subscription
To pipe an external Tokio `mpsc::Receiver` or socket stream into Iced:
```rust
fn ipc_subscription() -> Subscription<Message> {
    Subscription::run(|| {
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        tokio::spawn(async move {
            let mut server = start_server().await;
            while let Some(msg) = server.recv().await {
                if tx.send(Message::Ipc(msg)).await.is_err() {
                    break;
                }
            }
        });
        tokio_stream::wrappers::ReceiverStream::new(rx)
    })
}
```

### Window & Keyboard Events
Listen to raw window events via `event::listen_with`:
```rust
event::listen_with(|event, _status, window_id| {
    if let iced::Event::Window(window::Event::CloseRequested) = event {
        Some(Message::CloseRequested(window_id))
    } else {
        None
    }
})
```

---

## 5. Canvas & Immediate-Mode Graphics

For interactive drawing (selection boxes, graphs, custom graphics):
1. Implement `cosmic::iced::widget::canvas::Program<Message, Theme, Renderer>`.
2. In `update`, return `Action::request_redraw().and_capture()` on mouse drag for responsive 60 FPS drawing.
3. In `draw`, use `frame.fill_rectangle` and `frame.stroke` to render geometry:
   ```rust
   impl canvas::Program<SelectorMessage, Theme, Renderer> for SelectorProgram {
       type State = SelectorState;

       fn update(
           &self,
           state: &mut Self::State,
           event: &canvas::Event,
           bounds: Rectangle,
           cursor: mouse::Cursor,
       ) -> Option<canvas::Action<SelectorMessage>> {
           // Capture mouse drag and publish Selection message
       }

       fn draw(
           &self,
           state: &Self::State,
           renderer: &Renderer,
           theme: &Theme,
           bounds: Rectangle,
           cursor: mouse::Cursor,
       ) -> Vec<Geometry> {
           let mut frame = canvas::Frame::new(renderer, bounds.size());
           // Draw overlays, bounding boxes, or graphs
           vec![frame.into_geometry()]
       }
   }
   ```

---

## 6. View Purity & Performance Rules

- **Zero Allocations in View:** `view(&self)` is called on every redraw (up to 144 times/sec). Never parse JSON, format expensive strings, or allocate large vectors inside `view()`. Compute state in `update()` and read references in `view()`.
- **Cloning Messages:** Widget event callbacks require `Clone` on `Message`. Keep payloads lightweight (wrap large payloads in `Arc`).
