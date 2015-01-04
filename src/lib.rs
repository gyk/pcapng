#![feature(slicing_syntax, globs)]

use std::error::FromError;

use std::str::Utf8Error;
use std::string::FromUtf8Error;

use std::io::{Reader, BufReader, IoError};
use std::io::net::ip::IpAddr;

#[derive(Show)]
pub enum Error {
    Io(IoError),
    FormatError(FormatError)
}

#[derive(Show, Copy)]
pub enum FormatError {
    UnknownOption,
    Utf8Error(Utf8Error)
}

impl FromError<IoError> for Error {
    fn from_error(err: IoError) -> Error {
        Error::Io(err)
    }
}

impl FromError<FromUtf8Error> for Error {
    fn from_error(err: FromUtf8Error) -> Error {
        Error::FormatError(FormatError::Utf8Error(err.utf8_error()))
    }
}

impl FromError<FormatError> for Error {
    fn from_error(err: FormatError) -> Error {
        Error::FormatError(err)
    }
}

enum Block {
    SectionHeader(SectionHeaderBlock),
    InterfaceDescription(InterfaceDescriptionBlock),
    InterfaceStatistics(InterfaceStatisticsBlock),
    EnhancedPacket(EnhancedPacketBlock)
}

#[derive(Show)]
pub struct SectionHeaderBlock {
    /// Magic number equal to 0x1A2B3C4D. This field can be used to detect
    /// sections saved on systems with different endianness.
    pub magic: u32,

    pub major_version: u16,
    pub minor_version: u16,

    /// Length in bytes of the following section excluding this block.
    ///
    /// This block can be used to skip the section for faster navigation in
    /// large files. Length of -1u64 means that the length is unspecified.
    pub section_length: u64,
    pub options: Vec<SectionHeaderOption>
}

#[derive(Show)]
pub enum SectionHeaderOption {
    /// Comment associated with the current block
    Comment(String),

    /// Description of the hardware used to create this section
    Hardware(String),

    /// Name of the operating system used to create this section
    OS(String),

    /// Name of the application used to create this section
    UserApplication(String)
}

/// Block type 1 
#[derive(Show)]
pub struct InterfaceDescriptionBlock {
    /// Link layer type of the interface
    pub link_type: u16,

    /// Maximum number of bytes stored from each packet
    pub snap_len: u32,

    pub options: Vec<InterfaceDescriptionOption>
}

#[derive(Show)]
pub enum InterfaceDescriptionOption {
    /// Comment associated with the current block
    Comment(String),

    /// Name of the device used to capture data
    Name(String),

    /// Description of the device used to capture data
    Description(String),

    /// IPv4 interface address and netmask
    Ipv4Addr(u32, u32),

    /// IPv6 interface address and prefix length
    Ipv6Addr(IpAddr, u8),

    /// Hardware MAC address (48 bits)
    MacAddr(u64),

    /// Hardware EUI address
    EuiAddr(u64),

    /// Interface speed in bps
    Speed(u64),

    /// Resolution of timestamps.
    ///
    /// If the MSB is equal to zero, the remaining bits indicate the resolution
    /// as a negative power of 10 (e.g. 6 means microsecond resolution,
    /// timestamps are teh number of microseconds since 1/1/1970).
    ///
    /// If the MSB is equal to one, the remaining bits indicate the resolution
    /// as a negative power of 2 (e.g. 10 means 1/1024 of a second).
    ///
    /// If this options is not present, microsecond resolution is assumed.
    TsResolution(u8),

    /// Timezone for GMT support (?)
    Timezone(u32),

    /// The filter used to capture traffic. The first byte specifies the type
    /// of filter used (e.g. libpcap string, BPF bytecode, etc)
    Filter(u8, Vec<u8>),

    /// Operating system of the machine in which this interface is installed.
    /// This can be different from the Section Header Block if the capture
    /// was done remotely.
    OS(String),

    /// Length of Frame Check Sequence in bits for this interface. For link
    /// layers whose FCS length can change during time, the Packet Block
    /// flags be used.
    FcsLen(u8),

    /// Offset pf the timestamp in seconds 
    TsOffset(u64)
}

/// Block type 5
#[derive(Show)]
pub struct InterfaceStatisticsBlock {
    pub interface_id: u32,
    pub timestamp: u64,

    pub options: Vec<InterfaceStatisticsOption>
}

#[derive(Show)]
pub enum InterfaceStatisticsOption {
    /// Comment associated with the current block
    Comment(String),

    /// Time when capture started
    StartTime(u64),

    /// Time when capture ended
    EndTime(u64),

    /// Number of packets received from the physical interface
    Received(u64),

    /// Number of packets dropped by the interface due to the lack of resources
    Dropped(u64),
    
    /// Number of packets accepted by the filter
    FilterAccepted(u64),

    /// Number of packets dropped by the operating system
    OSDropped(u64),

    /// Number of packets delivered to the user starting. The value in this
    /// field can be different from 'Accepted - Dropped' because some packets
    /// could still be in the OS buffers when the capture ended.
    Delivered(u64)
}

#[derive(Show)]
pub struct EnhancedPacketBlock {
    pub interface_id: u32,
    pub timestamp: u64,

    /// Number of bytes captured from the packet
    pub captured_len: u32,

    /// Actual length of the packet when it was transmitted on the network
    pub len: u32,

    pub data: Vec<u8>,
    pub options: Vec<EnhancedPacketBlockOption>
}

#[derive(Show)]
pub enum EnhancedPacketBlockOption {
    Comment(String),
    Flags(u32),
    Hash,

    /// Number of packets lost between the last packet
    DropCount(u64)
}

#[inline(always)]
fn dword_aligned(n: uint) -> uint {
    (n + 3) & !3
}

pub fn read_block(r: &mut Reader) -> Result<(u32, Vec<u8>), IoError> {
    let block_type = try!(r.read_le_u32());

    let total_len = try!(r.read_le_u32());
    let data_len = total_len - 12; // 12 = type + 2*length

    let mut data = try!(r.read_exact(dword_aligned(data_len as uint)));
    data.truncate(data_len as uint);

    assert!(total_len == try!(r.read_le_u32()));

    Ok((block_type, data))
}

fn read_option(r: &mut Reader) -> Result<(u16, Vec<u8>), IoError> {
    let code = try!(r.read_le_u16());
    let len = try!(r.read_le_u16()) as uint;

    let mut data = try!(r.read_exact(dword_aligned(len)));
    data.truncate(len as uint);

    Ok((code, data))
}

impl SectionHeaderBlock {
    pub fn read(r: &mut BufReader) -> Result<SectionHeaderBlock, Error> {
        let magic = try!(r.read_le_u32());
    
        assert!(magic == 0x1A2B3C4D, "unsupported endianness");

        let major_version = try!(r.read_le_u16());
        let minor_version = try!(r.read_le_u16());
        let section_length = try!(r.read_le_u64());

        let mut options = Vec::new();

        if !r.eof() {
            loop {
                use SectionHeaderOption::*;

                let (code, data) = try!(read_option(r));

                let opt = match code {
                    0 => break,

                    1...4 => {
                        let s = try!(String::from_utf8(data));

                        match code {
                            1 => Comment(s),
                            2 => Hardware(s),
                            3 => OS(s),
                            4 => UserApplication(s),
                            _ => unreachable!()
                        }
                    }

                    _ => return Err(FromError::from_error(FormatError::UnknownOption))
                };

                options.push(opt);
            }
        }

        Ok(SectionHeaderBlock {
            magic: magic,
            major_version: major_version,
            minor_version: minor_version,
            section_length: section_length,

            options: options
        })
    }
}

impl InterfaceDescriptionBlock {
    pub fn read(r: &mut BufReader) -> Result<InterfaceDescriptionBlock, Error> {
        let link_type = try!(r.read_le_u16());
        try!(r.read_le_u16()); // reserved
        let snap_len = try!(r.read_le_u32());

        let mut options = Vec::new();

        if !r.eof() {
            loop {
                use InterfaceDescriptionOption::*;
                let (code, data) = try!(read_option(r));

                let mut d = &*data;

                let opt = match code {
                    0 => break,
                    1...3 | 12 => {
                        let s = try!(String::from_utf8(data.clone()));

                        match code {
                            1 => Comment(s),
                            2 => Name(s),
                            3 => Description(s),
                            12 => OS(s),
                            _ => unreachable!()
                        }
                    }

                    4 => {
                        let ip = try!(d.read_le_u32());
                        let mask = try!(d.read_le_u32());

                        Ipv4Addr(ip, mask)
                    }

                    6 => MacAddr(try!(d.read_le_uint_n(6))),
                    9 => TsResolution(try!(d.read_byte())),

                    _ => return Err(FromError::from_error(FormatError::UnknownOption))
                };

                options.push(opt);
            }
        }
    
        Ok(InterfaceDescriptionBlock {
            link_type: link_type,
            snap_len: snap_len,
            options: options
        })
    }
}

impl EnhancedPacketBlock {
    pub fn read(r: &mut BufReader) -> Result<EnhancedPacketBlock, Error> {
        let interface_id = try!(r.read_le_u32());
        let ts = try!(r.read_le_u64());
        let cap_len = try!(r.read_le_u32());
        let len = try!(r.read_le_u32());

        let aligned_len = dword_aligned(cap_len as uint);

        let mut packet_data = try!(r.read_exact(aligned_len));
        packet_data.truncate(cap_len as uint);

        let mut options = Vec::new();

        if !r.eof() {
            loop {
                use EnhancedPacketBlockOption::*;
                let (code, data) = try!(read_option(r));

                let opt = match code {
                    0 => break,
                    1 => {
                        match String::from_utf8(data.clone()) {
                            Ok(r) => Comment(r),
                            Err(err) => return Err(FromError::from_error(err))
                        }
                    }

                    // TODO: rest of the options
                    _ => return Err(FromError::from_error(FormatError::UnknownOption))
                };

                options.push(opt);
            }
        }

        Ok(EnhancedPacketBlock {
            interface_id: interface_id,
            timestamp: ts,
            captured_len: cap_len,
            len: len,
            data: packet_data,
            options: options
        })
    }
}

impl InterfaceStatisticsBlock {
    pub fn read(r: &mut BufReader) -> Result<InterfaceStatisticsBlock, Error> {
        let interface_id = try!(r.read_le_u32());
        let ts = try!(r.read_le_u64());

        let mut options = Vec::new();

        if !r.eof() {
            loop {
                use InterfaceStatisticsOption::*;
                let (code, data) = try!(read_option(r));

                let opt = match code {
                    0 => break,
                    1 => {
                        match String::from_utf8(data.clone()) {
                            Ok(r) => Comment(r),
                            Err(err) => return Err(FromError::from_error(err))
                        }
                    }

                    2...8 => {
                        let data = try!((&*data).read_le_u64());

                        match code {
                            2 => StartTime(data),
                            3 => EndTime(data),
                            4 => Received(data),
                            5 => Dropped(data),
                            6 => FilterAccepted(data),
                            7 => OSDropped(data),
                            8 => Delivered(data),
                            _ => unreachable!()
                        }
                    }

                    _ => return Err(FromError::from_error(FormatError::UnknownOption))
                };

                options.push(opt);
            }
        }

        Ok(InterfaceStatisticsBlock {
            interface_id: interface_id,
            timestamp: ts,
            options: options
        })
    }
}