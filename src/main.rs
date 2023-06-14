use log::{debug, error, info, warn};
use simple_logger::SimpleLogger;
use std::collections::btree_map::Range;
use std::env;
use std::fmt::Error;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::mem;
use std::process::exit;
use std::vec;
#[macro_use]
extern crate prettytable;
use prettytable::{Cell, Row, Table};

const MAX_X: usize = 30;
const MAX_Y: usize = 30;
// 0x 00 00 xx yy
const HDR_SIZE: usize = 2;
const HDR_00_offset: usize = 0;
const HDR_X_offset: usize = 0;
const HDR_Y_offset: usize = 1;
struct TableParms<'a> {
    x_val: u32,
    y_val: u32,
    start: u32,
    size: usize,
    count: u32,
    data_slice: &'a [u8],
}
impl<'a> TableParms<'a> {
    fn get_x_axis(&self) -> Vec<u8> {
        self.data_slice[HDR_SIZE..(HDR_SIZE + self.x_val as usize)].to_vec()
    }
    fn get_y_axis(&self) -> Vec<u8> {
        let rest_of_slice: &[u8] = &self.data_slice[(HDR_SIZE + self.x_val as usize)..];
        let y_vec: Vec<_> = rest_of_slice
            .iter()
            .step_by((self.x_val + 1) as usize)
            .copied()
            .collect();
        y_vec
    }
    fn get_rows(&self) -> Vec<Vec<u8>> {
        let data = self.data_slice.clone();
        let rest_of_slice: &[u8] = &self.data_slice[(HDR_SIZE + self.x_val as usize)..];
        let mut the_rows: Vec<Vec<u8>> = Vec::new();

        for valu in 0..self.y_val {
            let target: usize = ((self.x_val + 1) * valu) as usize;
            let mut the_row: Vec<u8> =
                rest_of_slice[(target + 1)..(target + 1 + self.x_val as usize)].to_vec();
            the_rows.push(the_row);
        }
        the_rows
    }
    fn get_table_end(&self) -> u32 {
        (self.start + self.size as u32) as u32
    }
    fn check_table_validity(&self) -> &'a Result<(), &'static str> {
        let x_slice: &[u8] = &self.data_slice[HDR_SIZE..(HDR_SIZE + self.x_val as usize)];

        match test_axis_goes_increasing(x_slice, "X") {
            Ok(()) => {}
            Err(e) => {
                debug!("{}", e);
                return &Err("Table Invalid");
            }
        }
        debug!("VALID X Axis! {:?}", x_slice);
        let rest_of_slice: &[u8] = &self.data_slice[(HDR_SIZE + self.x_val as usize)..];
        debug!("rest of slice {:x?}", rest_of_slice);
        debug!("stepsize 0x{:x}", (self.y_val + 1));
        let y_axis: Vec<_> = rest_of_slice
            .iter()
            .step_by((self.x_val + 1) as usize)
            .copied()
            .collect();
        match test_axis_goes_increasing(y_axis.as_slice(), "Y") {
            Ok(()) => {}
            Err(e) => {
                debug!("{}", e);
                return &Err("Table Invalid");
            }
        }
        &Ok(())
    }

    fn print_table(&self) {
        let x_axis: Vec<u8> = self.get_x_axis();
        let mut y_axis: Vec<u8> = self.get_y_axis();
        let mut data_rows = self.get_rows();

        let mut pretty_table = Table::new();

        let mut x_axis_cells = Vec::new();
        x_axis_cells.push(Cell::new(" "));
        //initialize the "axis row of the table"
        for x_val in &x_axis {
            x_axis_cells.push(Cell::new(&x_val.to_string()));
        }
        let x_axis = Row::new(x_axis_cells);
        pretty_table.add_row(x_axis);

        let rows_iter = data_rows.iter_mut().enumerate();
        for (idx, data_row) in rows_iter {
            let mut data_row_cells = Vec::new();
            let y_val = y_axis[idx];
            data_row.insert(0, y_val);
            for data_val in data_row {
                data_row_cells.push(Cell::new(&data_val.to_string()));
            }
            pretty_table.add_row(Row::new(data_row_cells));
        }

        pretty_table.printstd();
    }
}

fn main() {
    simple_logger::init_with_level(log::Level::Info).unwrap();
    let args: Vec<String> = env::args().collect();
    let file_path: &String = &args[1];
    let output_file_path = &args[2];

    let bytes: Vec<u8> = get_vec_from_file(file_path).expect("File Read Failure");
    let mut output_file = File::create(output_file_path).expect("Ouput file couldn't open.");

    let mut table_so_skip = true;
    let mut skip_idx = 0;
    let mut prev_table_size: usize = 0;

    let mut max_address: usize = 0;

    let mut tables: Vec<TableParms> = Vec::new();
    let mut valid_count: u32 = 0;

    if args.len() > 3 {
        if &args[3][0..2] != "0x" {
            print! {"Invalid format for hex_string. Please Use 0x<VALUE>\n"};
            return;
        }
        max_address = usize::from_str_radix(&args[3][2..], 16)
            .expect("Invalid hex format for max range. Use 0xVALUE");
    } else {
        max_address = bytes.len();
    }
    //Search for a table at every byte in the entire binary.
    for offset in 0..max_address - 4 {
        //loop skipping logic so that we don't accidentally find tables within tables.
        //Which is sometimes likely on large tables.
        if table_so_skip {
            if skip_idx < prev_table_size {
                skip_idx += 1;
                continue;
            } else {
                table_so_skip = false;
                skip_idx = 0;
                prev_table_size = 0;
            }
        }
        let byte: u8 = bytes[offset];

        //Tables usually appear with the following 'format'. Values are number of bytes for each .
        // |-1-|-1-|--X---|----1----|--Y--|----1----|--Y--|
        // |X  |Y  |X-axis|Y-axis[0]|Row_0|Y-axis[1]|Row_1|.....Y-Axis[n]|Row_n
        if (usize::from(bytes[offset + HDR_X_offset]) < MAX_X)
            && (usize::from(bytes[offset + HDR_X_offset]) > 1)//Ignore tables smaller than 2x2. Too many false positives.
            && (usize::from(bytes[offset + HDR_Y_offset]) < MAX_Y)
            && (usize::from(bytes[offset + HDR_Y_offset]) > 1)
        {
            debug!("Eligible Table Location: 0x{:x}\n", offset);
            let x_val: u32 = bytes[offset + HDR_X_offset] as u32;
            let y_val: u32 = bytes[offset + HDR_Y_offset] as u32;
            debug!("Eligible Table x/y: 0x{:x} 0x{:x}\n", x_val, y_val);
            let table_start: u32 = offset as u32;
            let table_size: usize = calc_table_size(x_val, y_val);

            if offset + table_size > bytes.len() {
                continue;
            }

            debug!("Eligible Table Size: 0x{:x}\n", table_size);
            let table_end: usize = table_start as usize + table_size;

            let copy_of_bytes: Vec<u8> = bytes.clone();
            let table_slice: &[u8] = &bytes[offset..table_end];

            let table: TableParms = TableParms {
                x_val,
                y_val,
                start: table_start,
                size: table_size,
                count: valid_count,
                data_slice: table_slice,
            };

            let x_axis: Vec<u8> = table.get_x_axis();
            let mut y_axis: Vec<u8> = table.get_y_axis();
            let mut rows = table.get_rows();

            let res = table.check_table_validity();
            match res {
                Ok(v) => {
                    table_so_skip = true;
                    prev_table_size = table.size;
                    write!(
                        output_file,
                        "Valid Table! 0x{:x?} 0x{:x?} 0x{:x?} 0x{:x?} 0x{:x?}\n",
                        table.start,
                        (table.start as usize + table.size),
                        table.size,
                        table.x_val,
                        table.y_val,
                    )
                    .expect("file write failed!");

                    debug!("X-Axis{:x?}", x_axis);
                    debug!("Y-Axis{:x?}", y_axis);
                    debug!("Rows:\n {:x?}", rows);
                    valid_count = valid_count + 1;
                    info!(
                        "Valid Table! Address: 0x{:x?} End: 0x{:x?} Length: 0x{:x?} X-Axis: 0x{:x?} Y-Axis: 0x{:x?} \nCount: {:}",
                        table.start,
                        (table.start as usize + table.size),
                        table.size,
                        table.x_val,
                        table.y_val,
                        valid_count,
                    );
                    table.print_table();

                    tables.push(table);
                    print!("\n")
                }
                Err(e) => debug!("{}", e),
            }
        }
    }
    tables.sort_by(|a, b| a.x_val.cmp(&b.x_val));
    for table in tables {
        info!("Address: {:x?} Count: {}\n", table.start, table.count);
        table.print_table();
    }
    return;
}
fn calc_table_size(x_val: u32, y_val: u32) -> usize {
    let mut size: u32 = 0x0;
    // row/column values
    size = size + 2;
    //initial data row
    size = size + (x_val);
    //header bytes for each data row
    size = size + (y_val);
    //rows
    size = size + (x_val * y_val);
    //print!("size: {} (0x{:x}) \n", size, size);
    size as usize
}

fn get_vec_from_file(file_name: &String) -> std::io::Result<Vec<u8>> {
    let mut file = File::open(&file_name)?;
    let mut contents: Vec<u8> = Vec::new();
    file.read_to_end(&mut contents)?;
    Ok(contents)
}
fn test_axis_goes_increasing(axis: &[u8], axis_name: &str) -> Result<(), &'static str> {
    let mut prev_val: &u8 = &0;
    let mut first_zero = true;
    for val in axis {
        debug!("prev_val {:?}, new_val {:?}", prev_val, val);
        if val > prev_val || (prev_val == &0 && first_zero == true) {
            first_zero = false;
            prev_val = val;
            continue;
        } else {
            debug!("{:} Axis Invalid!", axis_name);
            return Err("Axis Invalid");
        }
    }
    Ok(())
}
