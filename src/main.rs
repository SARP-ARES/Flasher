use serialport::{available_ports, SerialPort, SerialPortInfo};
use serialport;
use core::time;
use std::thread;
use std::time::Duration;
use std::fs::File;
use std::io::{prelude::*, stdin, stdout};
use std::path::Path;
use rfd::FileDialog;
use indicatif::ProgressBar;
use colored::*;
use terminal_menu::{button, label, list, menu, mut_menu, run};
use clearscreen;


fn get_bootloaders() -> Vec<String> {
    let mut vec: Vec<String> = vec![];
    
    match available_ports() {
        Ok(ports) => {
            for port in ports {
                match port.port_type {
                    serialport::SerialPortType::UsbPort(info) => {
                        if info.vid == 0x3232 && info.pid == 0 {
                            vec.push(port.port_name);
                        }
                    },
                    _ => {}
                };
            }
        }
        Err(e) => {
            println!("Error (get_bootloaders) | {e}");
        }
    }
    return vec;
}

fn get_nodes() -> Vec<String> {
    let mut vec: Vec<String> = vec![];
    
    match available_ports() {
        Ok(ports) => {
            for port in ports {
                match port.port_type {
                    serialport::SerialPortType::UsbPort(info) => {
                        if info.vid == 0x3232 && info.pid > 0 {
                            vec.push(port.port_name);
                        }
                    },
                    _ => {}
                };
            }
        }
        Err(e) => {
            println!("Error (get_nodes) | {e}");
        }
    }
    return vec;
}

fn get_all() -> Vec<SerialPortInfo> {
    let vec: Vec<SerialPortInfo> = vec![];
    
    match available_ports() {
        Ok(ports) => {
            return ports
        }
        Err(e) => {
            println!("Error (get_nodes) | {e}");
        }
    }
    return vec;
}

fn get_nodes_and_bootloaders() -> Vec<String> {
    let mut vec: Vec<String> = vec![];
    
    match available_ports() {
        Ok(ports) => {
            for port in ports {
                match port.port_type {
                    serialport::SerialPortType::UsbPort(info) => {
                        if info.vid == 0x3232 {
                            vec.push(port.port_name);
                        }
                    },
                    _ => {}
                };
            }
        }
        Err(e) => {
            println!("Error (get_nodes) | {e}");
        }
    }
    return vec;
}

#[derive(Debug)]
pub enum BootError {
    PortOpenError,
    SerialReadError,
    DTRError,
    WriteError,
    ReadError,
    FileOpenError,
    FileReadError,
}

#[derive(Debug)]
pub enum DebugError {
    PortOpenError,
    DTRError,
    ReadError,
}

fn get_file(path: &str) -> Option<String> {
    match FileDialog::new()
    .add_filter("binary", &["bin"])
    .set_directory(Path::new(path))
    .pick_file() {
        Some(n) => {
            match n.to_str() {
                Some(s) => {return Some(s.to_string())},
                None => {return None}
            }
        }
        None => None,
    }
}

// TODO: read_line can block forever if no \n is found, replace with other method
fn read_line(ser: &mut Box<dyn SerialPort>) -> Result<Option<String>, ()> {
    let mut out: String = String::new();
    let mut buf: [u8; 1] = [0];

    loop {
        match ser.read_exact(&mut buf) {
            Ok(_) => {
                if buf[0] as char == '\n' {
                    return Ok(Some(out));
                }
                out.push(buf[0] as char);
            }
            Err(e) => {
                if e.kind() != std::io::ErrorKind::TimedOut {
                    return Err(());
                }
            }
        }
    }
}

fn boot(port: String, path_name: String) -> Result<(), BootError> {
    let mut file = match File::open(Path::new(path_name.as_str())) {
        Ok(f) => f,
        Err(_) => {return Err(BootError::FileOpenError);}
    };

    let mut retry: u32 = 0;
    let mut ser = loop {
        match serialport::new(port.clone(), 115200)
            .timeout(Duration::from_millis(100))
            .open() {
            Ok(s) => break s,
            Err(_) => {
                retry += 1;
                if retry >= 30000 {
                    return Err(BootError::PortOpenError);
                }
            }
        };
    };
    

    match ser.write_data_terminal_ready(true) {
        Ok(_)  => {},
        Err(_) => {return Err(BootError::DTRError)}
    };
    match ser.write_all(b"BOOT") {
        Ok(_)  => {},
        Err(_) => {return Err(BootError::WriteError)}
    };
    match ser.flush() {
        Ok(_)  => {},
        Err(_) => {return Err(BootError::WriteError)}
    };

    let bar = ProgressBar::new(1000);

    let mut bin_index: u64 = 0;
    let mut size: u64 = 0;
    loop {
        match read_line(&mut ser) {
            Err(_) => {return Err(BootError::ReadError);}
            Ok(n) => {
                match n {
                    Some(s) => {
                        if s.contains("log:") {
                            bar.println(format!("> {}", &s[4..]));
                        }
                        else if s.contains("ICCID") {
                            bar.println(format!("{}", s.bold().green()));
                        }
                        else if s.contains("IMEI") {
                            bar.println(format!("{}", s.bold().green()));
                        }
                        else if s.contains("SID") {
                            bar.println(format!("{}", s.bold().green()));
                        }
                        else if s.contains("SIZE") {
                            size = match file.metadata() {
                                Ok(n) => n.len(),
                                Err(_) => {return Err(BootError::FileReadError);}
                            };

                            let sizestr = size.to_string();

                            match ser.write_all(sizestr.as_bytes()) {
                                Ok(_) => {},
                                Err(_) => {return Err(BootError::WriteError)}
                            };
                            match ser.flush() {
                                Ok(_) => {},
                                Err(_) => {return Err(BootError::WriteError)}
                            };
                        }
                        else if s.contains("DATA") {
                            let mut buf: [u8; 256] = [0; 256];

                            match file.seek(std::io::SeekFrom::Start(bin_index)) {
                                Ok(_) => {},
                                Err(_) => {return Err(BootError::FileReadError)}
                            }

                            match file.read(&mut buf) {
                                Ok(_) => {}
                                Err(_) => {return Err(BootError::FileReadError)}
                            };
                            match ser.write_all(&mut buf) {
                                Ok(_) => {bin_index += 256},
                                Err(_) => {}
                            };
                            let _ = ser.flush();

                            bar.set_position((1000f32 * bin_index as f32 / size as f32) as u64);
                            

                            //println!("{:.1}%", 100f32 * bin_index as f32 / size as f32);
                        }
                        else if s.contains("DONE") {
                            //println!("DONE");
                            bar.finish();
                            return Ok(());
                        }
                        else {
                            bar.println(format!("{} {}","> (uncaught)".red(), s.red()));
                        }
                    },
                    None => {}
                }
            }
        }
    
    }
}

fn debug(port: String) -> DebugError {
    let mut retry: u32 = 0;
    let mut ser = loop {
        match serialport::new(port.clone(), 115200)
            .timeout(Duration::from_millis(100))
            .open() {
            Ok(s) => break s,
            Err(_) => {
                retry += 1;
                if retry >= 30000 {
                    return DebugError::PortOpenError;
                }
            }
        };
    };
    match ser.write_data_terminal_ready(true) {
        Ok(_)  => {},
        Err(_) => {return DebugError::DTRError}
    };

    println!("{}", port.bold().blue());

    let mut lock = stdout().lock();
    let mut buf: [u8; 1] = [0];
    loop {
        match ser.read_exact(&mut buf) {
            Ok(_) => {
                let _ = write!(lock, "{}", buf[0] as char);
                let _ = ser.flush();
            }
            Err(e) => {
                if e.kind() != std::io::ErrorKind::TimedOut {
                    return DebugError::ReadError;
                }
            }
        }
    }
}

fn main() {
    let mut selected_file: String = String::new();
    let mut has_file: bool = false;
    loop {
        let menu = menu(vec![

            // label:
            //  not selectable, useful as a title, separator, etc...
            label("----------------------"),
            label("SSL Flash"),
            label("use wasd or arrow keys"),
            label("enter to select"),
            label("-----------------------"),
    
            // button:
            //  exit the menu
            button("Exit"),                                     //5
            button(format!("Select File | {}", selected_file)), //6
            button("Flash"),                                    //7
            button("Debug"),                                    //8
            list("Show Bootloader Logs", vec!["No", "Yes"])     //9
        ]);
        run(&menu);

        let bootloader: bool = match mut_menu(&menu).selection_value("Show Bootloader Logs") {
            "Yes" => true,
            _ => false,
        };

        match mut_menu(&menu).selected_item_index() {
            5 => {
                std::process::exit(0);
            }
            6 => {
                selected_file = match get_file("/") {
                    Some(s) => {
                        has_file = true;
                        s
                    },
                    None => {
                        has_file = false;
                        String::new()
                    },
                }
            }
            7 => {
                if !has_file {
                    println!("Error | No File Selected");
                }
                else {
                    loop {
                        let ports = get_bootloaders();
                
                        if ports.len() > 0 {
                            println!("booting: {}", ports[0].clone());
                            match boot(ports[0].clone(), selected_file.clone()) {
                                Ok(_) => {break;},
                                Err(e) => {
                                    println!("Error || {:?}", e);
                                    break;
                                }
                            }
                        }
                    } 
                }   
            
                print!("Press Enter to continue...");
                let _ = stdout().flush();
                let mut buffer = String::new();

                let _ = std::io::stdin().read_line(&mut buffer);
                let _ = clearscreen::clear();
            }
            8 => {
                let _ = clearscreen::clear();
                loop {
                    let ports = match bootloader {
                        true => get_nodes_and_bootloaders(),
                        false => get_nodes(),
                    };
            
                    if ports.len() > 0 {
                        match debug(ports[0].clone()) {
                            DebugError::PortOpenError => {},
                            DebugError::DTRError => {},
                            DebugError::ReadError => {
                                println!("{}", format!("End {}", ports[0].clone()).bold().red());
                            },
                        }
                    }
                }
            },
            _ => {},
        };

    }
    

    

    /*
    let f: String = loop {
        match get_file("/") {
            Some(s) => break s,
            None => {}
        }
    };

    loop {
        let ports = get_bootloaders();

        if ports.len() > 0 {
            println!("{:?}", ports);
            println!("{:?}", boot(ports[0].clone(), f.clone()));
        }
    }
    */
}
