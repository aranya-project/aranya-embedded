#[derive(Debug, Clone, Copy)]
pub enum Command {
    Get(Subject),
    Set(Subject),
}

#[derive(Debug, Clone, Copy)]
pub enum Subject {
    GraphId = 0,
    Sync = 1,
}

pub const PREFIX_LEN: usize = 3;

// Write prefix: 2 bytes length + 1 byte command
pub fn write_prefix(buffer: &mut [u8], length: u16, command: Command) -> usize {
    // Write length (16 bits)
    buffer[0] = (length >> 8) as u8;
    buffer[1] = length as u8;

    // Write command (8 bits)
    buffer[2] = match command {
        Command::Get(subject) => subject as u8,
        Command::Set(subject) => (subject as u8) | 0x80, // Set high bit for Set commands
    };

    PREFIX_LEN
}

// Read prefix and return (command, length to read next)
pub fn read_prefix(buffer: &[u8]) -> Option<(Command, u16)> {
    if buffer.len() < PREFIX_LEN {
        return None;
    }

    // Read length (first 2 bytes)
    let length = ((buffer[0] as u16) << 8) | (buffer[1] as u16);

    // Read command byte
    let is_set = (buffer[2] & 0x80) != 0;
    let subject_num = buffer[2] & 0x7F;

    let subject = match subject_num {
        0 => Some(Subject::GraphId),
        1 => Some(Subject::Sync),
        _ => None,
    }?;

    let command = if is_set {
        Command::Set(subject)
    } else {
        Command::Get(subject)
    };

    Some((command, length))
}
