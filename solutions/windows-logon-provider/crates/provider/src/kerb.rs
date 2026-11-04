//! Packing of `KERB_INTERACTIVE_UNLOCK_LOGON`.
//!
//! Derived from the Microsoft sample (MIT, see `THIRD-PARTY-NOTICES.md`): the
//! x86_64 layout is written by explicit offsets instead of stack pointers, so
//! the packer is safe Rust and host-tested. [`abi_checks`] verifies the offsets
//! against `ntsecapi.h` on the Windows target.
//!
//! All fields are little-endian:
//!
//! ```text
//! +0   KERB_INTERACTIVE_LOGON
//!        +0  MessageType: i32
//!        +8  LogonDomainName: UNICODE_STRING
//!        +24 UserName:        UNICODE_STRING
//!        +40 Password:        UNICODE_STRING
//! +56  LogonId: LUID (zeroed)
//! +64  domain UTF-16 bytes, user UTF-16 bytes, password UTF-16 bytes
//! ```
//!
//! Each `UNICODE_STRING` has `Length` and `MaximumLength` equal to the byte
//! length without the NUL terminator (like the Microsoft sample), and `Buffer`
//! holds a byte offset from the start of the packed buffer.

/// Size of `KERB_INTERACTIVE_UNLOCK_LOGON` on x86_64.
pub const UNLOCK_HEADER_BYTES: usize = 64;
/// Offset of `KERB_INTERACTIVE_LOGON.LogonDomainName`.
pub const DOMAIN_FIELD_OFFSET: usize = 8;
/// Offset of `KERB_INTERACTIVE_LOGON.UserName`.
pub const USER_FIELD_OFFSET: usize = 24;
/// Offset of `KERB_INTERACTIVE_LOGON.Password`.
pub const PASSWORD_FIELD_OFFSET: usize = 40;
/// Offset of `UNICODE_STRING.Buffer` inside a `UNICODE_STRING`.
pub const UNICODE_STRING_BUFFER_OFFSET: usize = 8;

/// `KerbInteractiveLogon` from `ntsecapi.h`.
pub const KERB_INTERACTIVE_LOGON: i32 = 2;
/// `KerbWorkstationUnlockLogon` from `ntsecapi.h`.
pub const KERB_WORKSTATION_UNLOCK_LOGON: i32 = 7;

/// Total size of the packed buffer, or `None` when a string does not fit the
/// `USHORT` byte length of `UNICODE_STRING` (the packer fails with
/// `HRESULT_FROM_WIN32(ERROR_ARITHMETIC_OVERFLOW)`).
pub fn packed_len(domain: &[u16], user: &[u16], password: &[u16]) -> Option<usize> {
    let domain_bytes = utf16_bytes(domain)?;
    let user_bytes = utf16_bytes(user)?;
    let password_bytes = utf16_bytes(password)?;
    let total = UNLOCK_HEADER_BYTES + domain_bytes + user_bytes + password_bytes;
    // `CREDENTIAL_PROVIDER_CREDENTIAL_SERIALIZATION.cbSerialization` is a DWORD.
    (total <= u32::MAX as usize).then_some(total)
}

fn utf16_bytes(text: &[u16]) -> Option<usize> {
    let bytes = text.len().checked_mul(2)?;
    (bytes <= u16::MAX as usize).then_some(bytes)
}

/// Builds the buffer that LSA consumes. Returns `None` on the same overflow
/// conditions as [`packed_len`].
pub fn pack(domain: &[u16], user: &[u16], password: &[u16], message_type: i32) -> Option<Vec<u8>> {
    let total = packed_len(domain, user, password)?;
    let mut buffer = vec![0u8; total];

    put_i32(&mut buffer, 0, message_type);
    // LogonId stays zeroed, exactly like `ZeroMemory(&pkiulOut->LogonId, ...)`.

    let mut cursor = UNLOCK_HEADER_BYTES;
    cursor = append_unicode_string(&mut buffer, DOMAIN_FIELD_OFFSET, domain, cursor);
    cursor = append_unicode_string(&mut buffer, USER_FIELD_OFFSET, user, cursor);
    let _ = append_unicode_string(&mut buffer, PASSWORD_FIELD_OFFSET, password, cursor);

    Some(buffer)
}

fn append_unicode_string(out: &mut [u8], field: usize, text: &[u16], cursor: usize) -> usize {
    let bytes = text.len() * 2;
    put_u16(out, field, bytes as u16); // Length
    put_u16(out, field + 2, bytes as u16); // MaximumLength (no NUL)
    put_u64(out, field + UNICODE_STRING_BUFFER_OFFSET, cursor as u64); // Buffer offset
    for (index, unit) in text.iter().enumerate() {
        let at = cursor + index * 2;
        out[at..at + 2].copy_from_slice(&unit.to_le_bytes());
    }
    cursor + bytes
}

fn put_u16(out: &mut [u8], at: usize, value: u16) {
    out[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(out: &mut [u8], at: usize, value: u64) {
    out[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

fn put_i32(out: &mut [u8], at: usize, value: i32) {
    out[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

/// Compile-time proof that the explicit offsets above match `ntsecapi.h` on the
/// real Windows target.
#[cfg(windows)]
mod abi_checks {
    use crate::bindings::Windows::Win32::Security::Authentication::Identity::{
        KERB_INTERACTIVE_LOGON as WinKerbInteractiveLogon,
        KERB_INTERACTIVE_UNLOCK_LOGON as WinKerbInteractiveUnlockLogon, LSA_UNICODE_STRING,
    };

    const _: () = {
        use core::mem::{offset_of, size_of};

        assert!(size_of::<WinKerbInteractiveUnlockLogon>() == super::UNLOCK_HEADER_BYTES);
        assert!(offset_of!(WinKerbInteractiveUnlockLogon, Logon) == 0);
        assert!(offset_of!(WinKerbInteractiveUnlockLogon, LogonId) == 56);
        assert!(offset_of!(WinKerbInteractiveLogon, MessageType) == 0);
        assert!(offset_of!(WinKerbInteractiveLogon, LogonDomainName) == super::DOMAIN_FIELD_OFFSET);
        assert!(offset_of!(WinKerbInteractiveLogon, UserName) == super::USER_FIELD_OFFSET);
        assert!(offset_of!(WinKerbInteractiveLogon, Password) == super::PASSWORD_FIELD_OFFSET);
        assert!(size_of::<LSA_UNICODE_STRING>() == 16);
        assert!(offset_of!(LSA_UNICODE_STRING, Buffer) == super::UNICODE_STRING_BUFFER_OFFSET);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    fn read_u16(buffer: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([buffer[at], buffer[at + 1]])
    }

    fn read_u64(buffer: &[u8], at: usize) -> u64 {
        u64::from_le_bytes(buffer[at..at + 8].try_into().unwrap())
    }

    #[test]
    fn size_is_header_plus_utf16_payloads() {
        let domain = utf16(".");
        let user = utf16("user");
        let password = utf16("p@ss");
        assert_eq!(
            packed_len(&domain, &user, &password),
            Some(UNLOCK_HEADER_BYTES + 2 + 8 + 8)
        );
        assert_eq!(packed_len(&[], &[], &[]), Some(UNLOCK_HEADER_BYTES));
    }

    #[test]
    fn strings_cannot_overflow_the_ushort_length() {
        // 0x8000 units = 0x1_0000 bytes, one past USHORT::MAX.
        let too_long = vec![0u16; 0x8000];
        assert_eq!(packed_len(&too_long, &[], &[]), None);
        assert!(pack(&too_long, &[], &[], KERB_INTERACTIVE_LOGON).is_none());

        // Exactly USHORT::MAX bytes still fits.
        let longest = vec![0u16; 0x7fff];
        assert_eq!(
            packed_len(&longest, &[], &[]),
            Some(UNLOCK_HEADER_BYTES + 0xfffe)
        );
    }

    #[test]
    fn layout_matches_the_unlock_structure() {
        let domain = utf16(".");
        let user = utf16("user");
        let password = utf16("secret");
        let packed = pack(&domain, &user, &password, KERB_INTERACTIVE_LOGON).unwrap();

        assert_eq!(packed.len(), UNLOCK_HEADER_BYTES + 2 + 8 + 12);

        assert_eq!(&packed[0..4], &KERB_INTERACTIVE_LOGON.to_le_bytes());
        assert!(packed[56..64].iter().all(|&b| b == 0));

        assert_eq!(read_u16(&packed, DOMAIN_FIELD_OFFSET), 2);
        assert_eq!(read_u16(&packed, DOMAIN_FIELD_OFFSET + 2), 2);
        assert_eq!(
            read_u64(&packed, DOMAIN_FIELD_OFFSET + UNICODE_STRING_BUFFER_OFFSET) as usize,
            UNLOCK_HEADER_BYTES
        );
        assert_eq!(read_u16(&packed, USER_FIELD_OFFSET), 8);
        assert_eq!(
            read_u64(&packed, USER_FIELD_OFFSET + UNICODE_STRING_BUFFER_OFFSET) as usize,
            UNLOCK_HEADER_BYTES + 2
        );
        assert_eq!(read_u16(&packed, PASSWORD_FIELD_OFFSET), 12);
        assert_eq!(
            read_u64(
                &packed,
                PASSWORD_FIELD_OFFSET + UNICODE_STRING_BUFFER_OFFSET
            ) as usize,
            UNLOCK_HEADER_BYTES + 2 + 8
        );

        let payload = &packed[UNLOCK_HEADER_BYTES..];
        assert_eq!(&payload[0..2], by_units(&domain).as_slice());
        assert_eq!(&payload[2..10], by_units(&user).as_slice());
        assert_eq!(&payload[10..22], by_units(&password).as_slice());
    }

    fn by_units(text: &[u16]) -> Vec<u8> {
        text.iter().flat_map(|unit| unit.to_le_bytes()).collect()
    }

    #[test]
    fn unlock_scenario_uses_the_workstation_message_type() {
        let packed = pack(
            &utf16("."),
            &utf16("user"),
            &utf16("pw"),
            KERB_WORKSTATION_UNLOCK_LOGON,
        )
        .unwrap();
        assert_eq!(&packed[0..4], &KERB_WORKSTATION_UNLOCK_LOGON.to_le_bytes());
        assert_eq!(
            7i32.to_le_bytes(),
            KERB_WORKSTATION_UNLOCK_LOGON.to_le_bytes()
        );
    }

    #[test]
    fn empty_strings_still_get_a_buffer_offset() {
        let packed = pack(&[], &[], &[], KERB_INTERACTIVE_LOGON).unwrap();
        assert_eq!(packed.len(), UNLOCK_HEADER_BYTES);
        for field in [
            DOMAIN_FIELD_OFFSET,
            USER_FIELD_OFFSET,
            PASSWORD_FIELD_OFFSET,
        ] {
            assert_eq!(read_u16(&packed, field), 0);
            assert_eq!(read_u16(&packed, field + 2), 0);
            assert_eq!(
                read_u64(&packed, field + UNICODE_STRING_BUFFER_OFFSET) as usize,
                UNLOCK_HEADER_BYTES
            );
        }
    }

    #[test]
    fn non_ascii_uses_utf16_units() {
        let domain = utf16(".");
        let user = utf16("пользователь");
        let packed = pack(&domain, &user, &[], KERB_INTERACTIVE_LOGON).unwrap();
        assert_eq!(
            read_u16(&packed, USER_FIELD_OFFSET) as usize,
            user.len() * 2
        );
        let start = UNLOCK_HEADER_BYTES + 2;
        assert_eq!(
            &packed[start..start + user.len() * 2],
            by_units(&user).as_slice()
        );
    }
}
