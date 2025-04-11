use std::cell::RefCell;
use std::rc::Rc;
use windows::core::Result;
use crate::mocks::sap_mocks::*;

// Traits to make our mocks compatible with the real SAP GUI components

// Trait for GuiComponent
pub trait GuiComponent {
    fn r_type(&self) -> Result<String>;
    fn name(&self) -> Result<String>;
}

impl GuiComponent for Rc<RefCell<MockComponent>> {
    fn r_type(&self) -> Result<String> {
        Ok(self.borrow().r_type.clone())
    }

    fn name(&self) -> Result<String> {
        Ok(self.borrow().name.clone())
    }
}

// Implement GuiSession for MockGuiSession
pub trait GuiSession {
    fn find_by_id(&self, id: String) -> Result<Rc<RefCell<MockComponent>>>;
    fn info(&self) -> Result<MockGuiSessionInfo>;
    fn start_transaction(&self, transaction: String) -> Result<()>;
    fn end_transaction(&self) -> Result<()>;
}

impl GuiSession for MockGuiSession {
    fn find_by_id(&self, id: String) -> Result<Rc<RefCell<MockComponent>>> {
        self.find_by_id(id)
    }

    fn info(&self) -> Result<MockGuiSessionInfo> {
        self.info()
    }

    fn start_transaction(&self, transaction: String) -> Result<()> {
        self.start_transaction(transaction)
    }

    fn end_transaction(&self) -> Result<()> {
        self.end_transaction()
    }
}
