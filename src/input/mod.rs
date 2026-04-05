pub mod enigo_input;
pub mod keysym;

#[derive(Debug, Clone, Copy)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

pub trait InputHandler {
    fn move_mouse(&mut self, x: u16, y: u16);
    fn mouse_button(&mut self, button: u8, pressed: bool);
    fn scroll(&mut self, direction: ScrollDirection);
    fn key_event(&mut self, keysym: u32, down: bool);
}

pub struct NoopInput;

impl InputHandler for NoopInput {
    fn move_mouse(&mut self, _x: u16, _y: u16) {}
    fn mouse_button(&mut self, _button: u8, _pressed: bool) {}
    fn scroll(&mut self, _direction: ScrollDirection) {}
    fn key_event(&mut self, _keysym: u32, _down: bool) {}
}
