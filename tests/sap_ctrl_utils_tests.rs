use windows::core::Result;
use sap_automation::utils::sap_interfaces::{exist_ctrl, hit_ctrl};
use sap_automation::utils::sap_mock_impl::create_test_session;

#[test]
fn test_exist_ctrl_found() -> Result<()> {
    let session = create_test_session();
    
    // Test finding a component that exists
    let result = exist_ctrl(&*session, 0, "/usr/txtField", true)?;
    
    assert!(result.cband, "Component should be found");
    assert_eq!(result.ctext, "Test Text", "Text should match");
    assert_eq!(result.ctype, "GuiTextField", "Type should match");
    
    Ok(())
}

#[test]
fn test_exist_ctrl_not_found() -> Result<()> {
    let session = create_test_session();
    
    // Test finding a component that doesn't exist
    let result = exist_ctrl(&*session, 0, "/usr/nonexistentField", true)?;
    
    assert!(!result.cband, "Component should not be found");
    assert_eq!(result.ctext, "", "Text should be empty");
    assert_eq!(result.ctype, "", "Type should be empty");
    
    Ok(())
}

#[test]
fn test_exist_ctrl_different_window() -> Result<()> {
    let session = create_test_session();
    
    // Test finding a component in a different window
    let result = exist_ctrl(&*session, 1, "", true)?;
    
    assert!(result.cband, "Component should be found");
    assert_eq!(result.ctext, "Popup Window", "Text should match");
    assert_eq!(result.ctype, "GuiFrameWindow", "Type should match");
    
    Ok(())
}

#[test]
fn test_hit_ctrl_get_text() -> Result<()> {
    let session = create_test_session();
    
    // Test getting text from a text field
    let result = hit_ctrl(&*session, 0, "/usr/txtField", "Text", "Get", "")?;
    
    assert_eq!(result, "Test Text", "Text should match");
    
    Ok(())
}

#[test]
fn test_hit_ctrl_set_text() -> Result<()> {
    let session = create_test_session();
    
    // Test setting text on a text field
    let _ = hit_ctrl(&*session, 0, "/usr/txtField", "Text", "Set", "New Text")?;
    
    // Verify the text was set by getting it again
    let result = hit_ctrl(&*session, 0, "/usr/txtField", "Text", "Get", "")?;
    assert_eq!(result, "New Text", "Text should be updated");
    
    Ok(())
}

#[test]
fn test_hit_ctrl_press_button() -> Result<()> {
    let session = create_test_session();
    
    // Test pressing a button
    // Note: We can't easily verify the button was pressed in our mock implementation,
    // but we can at least verify the function doesn't error
    let result = hit_ctrl(&*session, 0, "/tbar[0]/btn[0]", "Press", "", "")?;
    
    assert_eq!(result, "", "Result should be empty string");
    
    Ok(())
}

#[test]
fn test_hit_ctrl_checkbox() -> Result<()> {
    let session = create_test_session();
    
    // Test getting checkbox state
    let result = hit_ctrl(&*session, 0, "/usr/chkBox", "Selected", "Get", "")?;
    assert_eq!(result, "false", "Checkbox should be unchecked");
    
    // Test setting checkbox state
    let _ = hit_ctrl(&*session, 0, "/usr/chkBox", "Selected", "Set", "True")?;
    
    // Verify the checkbox state was set
    let result = hit_ctrl(&*session, 0, "/usr/chkBox", "Selected", "Get", "")?;
    assert_eq!(result, "true", "Checkbox should be checked");
    
    Ok(())
}

#[test]
fn test_hit_ctrl_statusbar() -> Result<()> {
    let session = create_test_session();
    
    // Test getting text from statusbar
    let result = hit_ctrl(&*session, 0, "/sbar", "Text", "Get", "")?;
    
    assert_eq!(result, "Status: OK", "Statusbar text should match");
    
    Ok(())
}
