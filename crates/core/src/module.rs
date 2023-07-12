use crate::{linker::Linker, BinaryFormat, InstrMeta, JumpDest, OpCode, WazmError, WazmResult};
use std::{collections::BTreeMap, io::Cursor};

pub struct CompiledModule {
    bytecode: Vec<OpCode>,
    metas: Vec<InstrMeta>,
    linker: Linker,
    num_globals: u32,
}

impl CompiledModule {
    pub fn from_vec(sink: &Vec<u8>) -> WazmResult<CompiledModule> {
        CompiledModule::from_cursor(Cursor::new(sink))
    }

    pub fn from_slice(sink: &[u8]) -> WazmResult<CompiledModule> {
        CompiledModule::from_cursor(Cursor::new(sink))
    }

    pub fn from_cursor(mut sink: Cursor<&[u8]>) -> WazmResult<CompiledModule> {
        let mut bytecode = Vec::new();
        let mut metas = Vec::new();

        // here we store mapping from jump destination to the opcode offset
        let mut jump_dest = BTreeMap::new();

        // read all opcodes from binary
        while sink.position() < sink.get_ref().len() as u64 {
            let offset = sink.position();
            let code = sink.get_ref()[0];

            let instr = OpCode::read_binary(&mut sink)?;
            println!("{:#04x}: {:?}", offset, instr);

            jump_dest.insert(offset as i32, bytecode.len());
            bytecode.push(instr);
            metas.push(InstrMeta(offset as usize, code));
        }
        println!();

        // if instruction has jump offset then its br-like and we should re-write jump offset
        for (index, opcode) in bytecode.iter_mut().enumerate() {
            if let Some(jump_offset) = opcode.get_jump_offset() {
                let relative_offset = jump_dest.get(&jump_offset.0).ok_or(WazmError::ReachedUnreachable)?;
                *opcode = opcode.rewrite_jump_offset(JumpDest::from(*relative_offset as i32 - index as i32));
            }
        }

        let num_globals = bytecode
            .iter()
            .filter_map(|opcode| match opcode {
                OpCode::GlobalGet(index) | OpCode::GlobalSet(index) => Some(index.0),
                _ => None,
            })
            .max()
            .map(|v| v + 1)
            .unwrap_or_default();

        Ok(CompiledModule {
            bytecode,
            metas,
            linker: Linker::new(),
            num_globals,
        })
    }

    pub fn linker_mut(&mut self) -> &mut Linker {
        &mut self.linker
    }

    pub fn linker(&self) -> &Linker {
        &self.linker
    }

    pub fn bytecode(&self) -> &Vec<OpCode> {
        &self.bytecode
    }

    pub fn metas(&self) -> &Vec<InstrMeta> {
        &self.metas
    }

    pub fn num_globals(&self) -> u32 {
        self.num_globals
    }

    pub fn trace_binary(&self) -> String {
        let mut result = String::new();
        for opcode in self.bytecode().iter() {
            let str = format!("{:?}\n", opcode);
            result += str.as_str();
        }
        result
    }
}
