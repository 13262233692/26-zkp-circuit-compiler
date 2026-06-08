use num_bigint::BigUint;
use num_traits::Zero;
use std::io::{self, Write};

use crate::error::{CompileError, Result};
use crate::r1cs::R1csSystem;

const R1CS_MAGIC: &[u8; 4] = b"r1cs";
const R1CS_VERSION: u32 = 1;
const SECTION_HEADER: u32 = 0;
const SECTION_CONSTRAINTS: u32 = 1;
const SECTION_WIRE_MAP: u32 = 2;
const SECTION_LABEL_MAP: u32 = 3;
const FIELD_SIZE: u32 = 32;

fn write_u32(w: &mut impl Write, val: u32) -> io::Result<()> {
    w.write_all(&val.to_le_bytes())
}

fn write_u64(w: &mut impl Write, val: u64) -> io::Result<()> {
    w.write_all(&val.to_le_bytes())
}

fn write_field_element(w: &mut impl Write, val: &BigUint, field_size: u32) -> io::Result<()> {
    let bytes = val.to_bytes_be();
    let sz = field_size as usize;
    let mut buf = vec![0u8; sz];
    let offset = sz - bytes.len();
    buf[offset..].copy_from_slice(&bytes);
    w.write_all(&buf)
}

fn write_lc(
    w: &mut impl Write,
    lc: &crate::r1cs::LinearCombination,
    field_size: u32,
) -> io::Result<()> {
    let non_zero: Vec<_> = lc.terms.iter().filter(|(_, v)| !v.is_zero()).collect();
    write_u32(w, non_zero.len() as u32)?;
    for (idx, coeff) in &non_zero {
        write_u32(w, **idx as u32)?;
        write_field_element(w, coeff, field_size)?;
    }
    Ok(())
}

pub fn serialize(system: &R1csSystem, writer: &mut impl Write) -> Result<()> {
    writer.write_all(R1CS_MAGIC)?;
    write_u32(writer, R1CS_VERSION)?;

    let num_sections: u32 = 4;
    write_u32(writer, num_sections)?;

    let mut header_data = Vec::new();
    write_u32(&mut header_data, FIELD_SIZE)?;
    write_field_element(&mut header_data, &system.prime, FIELD_SIZE)?;
    write_u32(&mut header_data, system.num_variables as u32)?;
    write_u32(&mut header_data, system.num_public_outputs as u32)?;
    write_u32(&mut header_data, system.num_public_inputs as u32)?;
    write_u32(&mut header_data, system.num_private_inputs as u32)?;
    write_u64(&mut header_data, system.num_variables as u64)?;
    write_u32(&mut header_data, system.constraints.len() as u32)?;

    write_u32(writer, SECTION_HEADER)?;
    write_u64(writer, header_data.len() as u64)?;
    writer.write_all(&header_data)?;

    let mut constraint_data = Vec::new();
    for constraint in &system.constraints {
        write_lc(&mut constraint_data, &constraint.a, FIELD_SIZE)?;
        write_lc(&mut constraint_data, &constraint.b, FIELD_SIZE)?;
        write_lc(&mut constraint_data, &constraint.c, FIELD_SIZE)?;
    }

    write_u32(writer, SECTION_CONSTRAINTS)?;
    write_u64(writer, constraint_data.len() as u64)?;
    writer.write_all(&constraint_data)?;

    let mut wire_data = Vec::new();
    for i in 0..system.num_variables {
        let _ = write_u32(&mut wire_data, i as u32);
    }

    write_u32(writer, SECTION_WIRE_MAP)?;
    write_u64(writer, wire_data.len() as u64)?;
    writer.write_all(&wire_data)?;

    let mut label_data = Vec::new();
    for name in &system.variable_names {
        let name_bytes = name.as_bytes();
        write_u32(&mut label_data, name_bytes.len() as u32)?;
        label_data.extend_from_slice(name_bytes);
    }

    write_u32(writer, SECTION_LABEL_MAP)?;
    write_u64(writer, label_data.len() as u64)?;
    writer.write_all(&label_data)?;

    writer.flush()?;
    Ok(())
}

pub fn deserialize_and_inspect(data: &[u8]) -> Result<String> {
    if data.len() < 16 {
        return Err(CompileError::SerializeError {
            message: "file too short".to_string(),
        });
    }

    if &data[0..4] != R1CS_MAGIC {
        return Err(CompileError::SerializeError {
            message: "invalid magic bytes".to_string(),
        });
    }

    let mut output = String::new();
    output.push_str(&format!("Magic: {}\n", String::from_utf8_lossy(&data[0..4])));

    let version = u32::from_le_bytes(data[4..8].try_into().unwrap());
    output.push_str(&format!("Version: {}\n", version));

    let num_sections = u32::from_le_bytes(data[8..12].try_into().unwrap());
    output.push_str(&format!("Number of sections: {}\n\n", num_sections));

    let mut offset = 12;

    for _sect in 0..num_sections {
        if offset + 12 > data.len() {
            break;
        }
        let sect_type = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
        let sect_len = u64::from_le_bytes(data[offset + 4..offset + 12].try_into().unwrap()) as usize;
        offset += 12;

        match sect_type {
            0 => {
                output.push_str("=== Header Section ===\n");
                let sect_data = &data[offset..offset + sect_len];
                let fs = u32::from_le_bytes(sect_data[0..4].try_into().unwrap());
                output.push_str(&format!("Field size: {} bytes\n", fs));

                let prime_bytes = &sect_data[4..4 + fs as usize];
                let prime = BigUint::from_bytes_be(prime_bytes);
                output.push_str(&format!("Prime: {}\n", prime));

                let mut p = 4 + fs as usize;
                let num_wires = u32::from_le_bytes(sect_data[p..p + 4].try_into().unwrap()) as usize;
                p += 4;
                let num_outputs = u32::from_le_bytes(sect_data[p..p + 4].try_into().unwrap()) as usize;
                p += 4;
                let num_pub_inputs = u32::from_le_bytes(sect_data[p..p + 4].try_into().unwrap()) as usize;
                p += 4;
                let num_priv_inputs = u32::from_le_bytes(sect_data[p..p + 4].try_into().unwrap()) as usize;
                p += 4;
                let _num_labels = u64::from_le_bytes(sect_data[p..p + 8].try_into().unwrap());
                p += 8;
                let num_constraints = u32::from_le_bytes(sect_data[p..p + 4].try_into().unwrap()) as usize;

                output.push_str(&format!("Total wires: {}\n", num_wires));
                output.push_str(&format!("Public outputs: {}\n", num_outputs));
                output.push_str(&format!("Public inputs: {}\n", num_pub_inputs));
                output.push_str(&format!("Private inputs: {}\n", num_priv_inputs));
                output.push_str(&format!("Constraints: {}\n", num_constraints));
                output.push('\n');
            }
            1 => {
                output.push_str("=== Constraints Section ===\n");
                let _sect_data = &data[offset..offset + sect_len];
                output.push_str(&format!("Section size: {} bytes\n\n", sect_len));
            }
            2 => {
                output.push_str("=== Wire Map Section ===\n");
                output.push_str(&format!("Section size: {} bytes\n\n", sect_len));
            }
            3 => {
                output.push_str("=== Label Map Section ===\n");
                let sect_data = &data[offset..offset + sect_len];
                let mut p = 0;
                let mut idx = 0;
                while p < sect_data.len() {
                    if p + 4 > sect_data.len() {
                        break;
                    }
                    let name_len = u32::from_le_bytes(sect_data[p..p + 4].try_into().unwrap()) as usize;
                    p += 4;
                    if p + name_len > sect_data.len() {
                        break;
                    }
                    let name = String::from_utf8_lossy(&sect_data[p..p + name_len]);
                    output.push_str(&format!("  v{} = {}\n", idx, name));
                    p += name_len;
                    idx += 1;
                }
                output.push('\n');
            }
            _ => {
                output.push_str(&format!("=== Unknown Section {} ===\n", sect_type));
                output.push_str(&format!("Section size: {} bytes\n\n", sect_len));
            }
        }

        offset += sect_len;
    }

    Ok(output)
}
