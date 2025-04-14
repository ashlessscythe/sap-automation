use windows::core::Result;
use crate::utils::sap_constants::CtrlCheck;

/// Trait representing a SAP GUI component
pub trait SapComponent {
    /// Get the type of the component
    fn r_type(&self) -> Result<String>;
    
    /// Get the name of the component
    fn name(&self) -> Result<String>;
    
    /// Get the text of the component (if applicable)
    fn get_text(&self) -> Result<String>;
    
    /// Set the text of the component (if applicable)
    fn set_text(&self, text: String) -> Result<()>;
    
    /// Set focus to the component (if applicable)
    fn set_focus(&self) -> Result<()>;
    
    /// Press the component (if it's a button)
    fn press(&self) -> Result<()>;
    
    /// Select the component (if it's a radio button)
    fn select(&self) -> Result<()>;
    
    /// Get the selected state (if it's a checkbox)
    fn selected(&self) -> Result<bool>;
    
    /// Set the selected state (if it's a checkbox)
    fn set_selected(&self, selected: bool) -> Result<()>;
    
    /// Maximize the component (if it's a window)
    fn maximize(&self) -> Result<()>;
}

/// Trait representing a SAP GUI session
pub trait SapSession {
    /// Find a component by ID
    fn find_by_id(&self, id: String) -> Result<Box<dyn SapComponent>>;
    
    /// Get session information
    fn info(&self) -> Result<Box<dyn SapSessionInfo>>;
    
    /// Start a transaction
    fn start_transaction(&self, transaction: String) -> Result<()>;
    
    /// End a transaction
    fn end_transaction(&self) -> Result<()>;
}

/// Trait representing SAP GUI session information
pub trait SapSessionInfo {
    /// Get the current transaction
    fn transaction(&self) -> Result<String>;
}

/// Trait for a SAP component factory
pub trait SapComponentFactory {
    /// Create a new SAP session
    fn create_session(&self, name: &str) -> Box<dyn SapSession>;
}

/// Function to check if a control exists in the SAP GUI
pub fn exist_ctrl(session: &dyn SapSession, n_wnd: i32, control_id: &str, ret_msg: bool) -> Result<CtrlCheck> {
    let mut err_chk = CtrlCheck {
        cband: false,
        ctext: String::new(),
        ctype: String::new(),
    };
    
    // Try to find the control
    let control_path = format!("wnd[{}]{}", n_wnd, control_id);
    let ret_id = session.find_by_id(control_path.clone());
    
    if let Ok(component) = ret_id {
        err_chk.cband = true;
        
        if ret_msg {
            // Get type information
            err_chk.ctype = component.r_type().unwrap_or_default();
            
            // Get text from the component
            err_chk.ctext = component.get_text().unwrap_or_default();
        }
    }
    
    Ok(err_chk)
}

/// Function to interact with a control in the SAP GUI
pub fn hit_ctrl(session: &dyn SapSession, n_wnd: i32, control_id: &str, event_id: &str, event_id_opt: &str, event_id_value: &str) -> Result<String> {
    let mut aux_str = String::new();
    let control_path = format!("wnd[{}]{}", n_wnd, control_id);
    
    if let Ok(component) = session.find_by_id(control_path) {
        match event_id {
            "Maximize" => {
                component.maximize()?;
            },
            "Minimize" => {
                // Note: minimize is not available, using maximize as a fallback
                component.maximize()?;
            },
            "Press" => {
                component.press()?;
            },
            "Select" => {
                component.select()?;
            },
            "Selected" => {
                match event_id_opt {
                    "Get" => {
                        aux_str = component.selected()?.to_string();
                    },
                    "Set" => {
                        match event_id_value {
                            "True" => component.set_selected(true)?,
                            "False" => component.set_selected(false)?,
                            _ => {}
                        }
                    },
                    _ => {}
                }
            },
            "Focus" => {
                component.set_focus()?;
            },
            "Text" => {
                match event_id_opt {
                    "Get" => {
                        aux_str = component.get_text()?;
                    },
                    "Set" => {
                        component.set_text(event_id_value.to_string())?;
                    },
                    _ => {}
                }
            },
            _ => {}
        }
    }
    
    Ok(aux_str)
}
