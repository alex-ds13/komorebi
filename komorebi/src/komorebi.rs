use crate::border_manager::BorderManager;
use crate::WindowManager;


#[derive(Debug)]
pub struct Komorebi {
    pub window_manager: WindowManager,
    pub border_manager: BorderManager,
}
