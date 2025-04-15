use std::path::Path;
use std::io::{self, ErrorKind};
use calamine::{open_workbook, Reader, Xlsx, Range, DataType};
use anyhow::{Result, Context};

/// Represents a cell value from an Excel file
#[derive(Debug, Clone)]
pub enum ExcelValue {
    String(String),
    Float(f64),
    Int(i64),
    Bool(bool),
    Empty,
}

impl From<&DataType> for ExcelValue {
    fn from(dt: &DataType) -> Self {
        match *dt {
            DataType::String(ref s) => ExcelValue::String(s.clone()),
            DataType::Float(f) => ExcelValue::Float(f),
            DataType::Int(i) => ExcelValue::Int(i),
            DataType::Bool(b) => ExcelValue::Bool(b),
            DataType::Empty => ExcelValue::Empty,
            DataType::Error(_) => ExcelValue::Empty,
            DataType::DateTime(d) => ExcelValue::Float(d),
            DataType::Duration(d) => ExcelValue::Float(d),
            DataType::DateTimeIso(ref s) => ExcelValue::String(s.clone()),
            DataType::DurationIso(ref s) => ExcelValue::String(s.clone()),
        }
    }
}

impl ToString for ExcelValue {
    fn to_string(&self) -> String {
        match self {
            ExcelValue::String(s) => s.clone(),
            ExcelValue::Float(f) => f.to_string(),
            ExcelValue::Int(i) => i.to_string(),
            ExcelValue::Bool(b) => b.to_string(),
            ExcelValue::Empty => String::new(),
        }
    }
}

/// Represents a dataframe (table) from an Excel file
#[derive(Debug, Clone)]
pub struct ExcelDataFrame {
    pub headers: Vec<String>,
    pub data: Vec<Vec<ExcelValue>>,
}

impl ExcelDataFrame {
    /// Creates a new empty dataframe
    pub fn new() -> Self {
        ExcelDataFrame {
            headers: Vec::new(),
            data: Vec::new(),
        }
    }

    /// Gets a column by name
    pub fn get_column(&self, column_name: &str) -> Option<Vec<ExcelValue>> {
        let column_index = self.headers.iter().position(|h| h == column_name)?;
        Some(self.data.iter().map(|row| row[column_index].clone()).collect())
    }

    /// Gets multiple columns by name
    pub fn get_columns(&self, column_names: &[&str]) -> Option<Vec<Vec<ExcelValue>>> {
        let column_indices: Vec<usize> = column_names
            .iter()
            .filter_map(|&name| self.headers.iter().position(|h| h == name))
            .collect();

        if column_indices.len() != column_names.len() {
            return None;
        }

        Some(
            column_indices
                .iter()
                .map(|&idx| self.data.iter().map(|row| row[idx].clone()).collect())
                .collect(),
        )
    }

    /// Formats a column for SAP multi-value field (tab-separated values)
    pub fn format_column_for_sap(&self, column_name: &str) -> Option<String> {
        let column = self.get_column(column_name)?;
        Some(
            column
                .iter()
                .map(|val| val.to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<String>>()
                .join("\t"),
        )
    }

    /// Formats multiple columns for SAP multi-value field (tab-separated values)
    pub fn format_columns_for_sap(&self, column_names: &[&str]) -> Option<String> {
        let columns = self.get_columns(column_names)?;
        
        // Transpose the columns to rows
        let mut rows: Vec<Vec<String>> = Vec::new();
        if !columns.is_empty() {
            let row_count = columns[0].len();
            for row_idx in 0..row_count {
                let mut row = Vec::new();
                for col in &columns {
                    if row_idx < col.len() {
                        row.push(col[row_idx].to_string());
                    } else {
                        row.push(String::new());
                    }
                }
                rows.push(row);
            }
        }

        // Join rows with tabs and newlines
        Some(
            rows.iter()
                .map(|row| row.join("\t"))
                .filter(|s| !s.is_empty())
                .collect::<Vec<String>>()
                .join("\n"),
        )
    }
}

/// Reads an Excel file and returns a dataframe
pub fn read_excel_file(file_path: &str, sheet_name: &str) -> Result<ExcelDataFrame> {
    let path = Path::new(file_path);
    let mut workbook: Xlsx<_> = open_workbook(path)
        .with_context(|| format!("Failed to open Excel file: {}", file_path))?;
    
    let range = workbook.worksheet_range(sheet_name)
        .with_context(|| format!("Failed to read sheet '{}' from Excel file", sheet_name))?;

    parse_excel_range(range)
}

/// Parses an Excel range into a dataframe
fn parse_excel_range(range: Range<DataType>) -> Result<ExcelDataFrame> {
    let mut df = ExcelDataFrame::new();
    
    // Get dimensions
    let (height, width) = range.get_size();
    if height == 0 {
        return Ok(df);
    }

    // Extract headers from the first row
    for i in 0..width {
        if let Some(cell) = range.get_value((0, i as u32)) {
            df.headers.push(cell.to_string());
        } else {
            df.headers.push(format!("Column_{}", i + 1));
        }
    }

    // Extract data rows
    for row_idx in 1..height {
        let mut row = Vec::with_capacity(width);
        for col_idx in 0..width {
            if let Some(cell) = range.get_value((row_idx as u32, col_idx as u32)) {
                row.push(ExcelValue::from(cell));
            } else {
                row.push(ExcelValue::Empty);
            }
        }
        df.data.push(row);
    }

    Ok(df)
}

/// Reads specific columns from an Excel file
pub fn read_excel_columns(file_path: &str, sheet_name: &str, column_names: &[&str]) -> Result<Vec<Vec<ExcelValue>>> {
    let df = read_excel_file(file_path, sheet_name)?;
    
    df.get_columns(column_names)
        .ok_or_else(|| io::Error::new(ErrorKind::NotFound, 
            format!("One or more columns not found: {:?}", column_names)).into())
}

/// Reads a specific column from an Excel file
pub fn read_excel_column(file_path: &str, sheet_name: &str, column_name: &str) -> Result<Vec<ExcelValue>> {
    let df = read_excel_file(file_path, sheet_name)?;
    
    df.get_column(column_name)
        .ok_or_else(|| io::Error::new(ErrorKind::NotFound, 
            format!("Column not found: {}", column_name)).into())
}

/// Formats a specific column from an Excel file for SAP multi-value field
pub fn format_excel_column_for_sap(file_path: &str, sheet_name: &str, column_name: &str) -> Result<String> {
    let df = read_excel_file(file_path, sheet_name)?;
    
    df.format_column_for_sap(column_name)
        .ok_or_else(|| io::Error::new(ErrorKind::NotFound, 
            format!("Column not found: {}", column_name)).into())
}

/// Formats multiple columns from an Excel file for SAP multi-value field
pub fn format_excel_columns_for_sap(file_path: &str, sheet_name: &str, column_names: &[&str]) -> Result<String> {
    let df = read_excel_file(file_path, sheet_name)?;
    
    df.format_columns_for_sap(column_names)
        .ok_or_else(|| io::Error::new(ErrorKind::NotFound, 
            format!("One or more columns not found: {:?}", column_names)).into())
}

/// Reads an Excel file and returns it as a vector of vectors (rows)
pub fn read_excel_as_vec_of_vecs(file_path: &str, sheet_name: &str) -> Result<Vec<Vec<String>>> {
    let df = read_excel_file(file_path, sheet_name)?;
    
    let mut result = Vec::with_capacity(df.data.len());
    for row in &df.data {
        let string_row: Vec<String> = row.iter().map(|val| val.to_string()).collect();
        result.push(string_row);
    }
    
    Ok(result)
}
