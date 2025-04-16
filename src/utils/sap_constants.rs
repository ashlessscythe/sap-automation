use sap_scripting::*;

// Constants from VBA
pub const TIME_FORMAT: &str = "mm-dd-yy hh:mm:ss";
pub const STR_FORM: &str =
    "\n****************************************************************************\n";

// Window types from VBA
pub const WORD: &str = "OpusApp";
pub const EXCEL: &str = "XLMAIN";
pub const IEXPLORER: &str = "IEFrame";
pub const MSVBASIC: &str = "wndclass_desked_gsk";
pub const NOTEPAD: &str = "Notepad";

// Windows message constants
pub const WM_CLOSE: u32 = 0x10;

// Resource types from VBA
#[derive(Debug, Clone, Copy)]
pub enum Resource {
    Connected = 0x1,
    Remembered = 0x3,
    GlobalNet = 0x2,
}

#[derive(Debug, Clone, Copy)]
pub enum ResourceType {
    Disk = 0x1,
    Print = 0x2,
    Any = 0x0,
}

#[derive(Debug, Clone, Copy)]
pub enum ResourceViewType {
    Domain = 0x1,
    Generic = 0x0,
    Server = 0x2,
    Share = 0x3,
}

#[derive(Debug, Clone, Copy)]
pub enum ResourceUseType {
    Connectable = 0x1,
    Container = 0x2,
}

// Structs from VBA
#[derive(Debug, Clone)]
pub struct WndTitleCaption {
    pub wnd_type: String,
    pub wnd_title: String,
}

#[derive(Debug, Clone)]
pub struct ErrorCheck {
    pub bchgb: bool,
    pub msg: String,
}

#[derive(Debug, Clone)]
pub struct CtrlCheck {
    pub cband: bool,
    pub ctext: String,
    pub ctype: String,
}

#[derive(Debug, Clone)]
pub struct ParamsStruct {
    pub instance_id: String,
    pub client_id: String,
    pub user: String,
    pub pass: String,
    pub language: String,
}
