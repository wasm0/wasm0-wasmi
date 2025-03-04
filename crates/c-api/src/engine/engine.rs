use std::collections::HashMap;
use std::sync::{Mutex};
use wasmi::{AsContext, AsContextMut, Config, Engine, Error, ExternType, Func, Instance, IntoFunc, Linker, Module, OpCodeState, ResumableCall, Store, TypedResumableCall};
use wasmi::core::Trap;
use wasmi::ResumableCall::Resumable;

#[derive(Debug)]
pub struct WasmEngine {
    config: Config,
    store: Store<()>,
    engine: Engine,
    wasm_binary: Option<Vec<u8>>,
    host_fns: HashMap<String, Func>,
    lock: Mutex<i32>,
    instance: Option<Instance>,
    // memory_data_ptr: *mut u8,
}

unsafe impl Sync for WasmEngine {}

impl WasmEngine {
    pub fn new(wasm_binary: Option<Vec<u8>>) -> Result<Self, Error> {
        let mut config = Config::default();
        config.consume_fuel(false);
        let engine = Engine::new(&config);
        let store = Store::new(&engine, ());

        let res = Self {
            config,
            store,
            engine,
            wasm_binary,
            host_fns: HashMap::new(),
            lock: Mutex::new(0),
            instance: None,
            // memory_data_ptr: null_mut(),
        };

        Ok(res)
    }

    pub fn set_wasm(&mut self, wasm_binary: &Vec<u8>) {
        match self.lock.lock() {
            Ok(_) => {
                self.wasm_binary = Some(wasm_binary.clone());
            },
            Err(_) => panic!("lock failed")
        }
        self.init_module().unwrap();
    }

    fn init_module(&mut self) -> Result<(), Error> {
        let mut linker = Linker::<()>::new(&self.engine);
        let module = Module::new(self.store.engine(), self.wasm_binary.as_ref().unwrap().as_slice()).unwrap();
        for (n, f) in self.host_fns.iter() {
            linker.define("env", n.as_ref(), *f)?;
        }
        let instance = linker
            .instantiate(&mut self.store, &module)
            .unwrap()
            .start(&mut self.store)
            .unwrap();
        self.instance = Some(instance);

        // self.init_memory_data_ptr();

        Ok(())
    }

    // fn init_memory_data_ptr(&mut self) {
    //     let mut memory: Option<Memory> = None;
    //     let instance = self.instance.unwrap();
    //     for export in instance.exports(&self.store) {
    //         if export.name() != "memory" { continue }
    //         if let ExternType::Memory(_) = export.ty(&self.store) {
    //             let export = instance.get_export(&self.store, export.name());
    //             if let Some(export) = export {
    //                 memory = export.into_memory();
    //             }
    //         }
    //     }
    //     if let Some(memory) = memory {
    //         let mem_data = memory.data_mut(&mut self.store).as_mut_ptr();
    //         self.memory_data_ptr = mem_data;
    //     }
    // }

    pub fn compute_result(&mut self) -> Result<i32, Error> {
        let func;
        match self.lock.lock() {
            Ok(_) => {
                let instance = self.instance.unwrap();
                let f = instance.get_func(&self.store, "main").unwrap();
                func = f.typed::<(), ()>(&self.store).unwrap();
            },
            Err(_) => panic!("lock failed")
        }
        // do not lock the lines below: wasm calls host functions which may call back to wasmi containing lock
        let call = func.call_resumable(&mut self.store, ()).unwrap();
        match call {
            TypedResumableCall::Finished(_) => {}
            TypedResumableCall::Resumable(invocation) => {
                let host_err = invocation.host_error();
                return Ok(host_err.i32_exit_status().unwrap());
            }
        }
        Ok(0)
    }

    pub fn dump_trace(&mut self) -> Result<String, Error> {
        let json_body = match self.lock.lock() {
            Ok(_) => {
                self.store.tracer.to_json()
            },
            Err(_) => panic!("lock failed")
        };
        Ok(json_body)
    }

    pub fn get_last_pc(&mut self) -> Option<u32> {
        self.store.tracer.get_last_pc()
    }


    pub fn compute_trace(&mut self) -> Result<String, Error> {
        let func;
        match self.lock.lock() {
            Ok(_) => {
                let instance = self.instance.unwrap();
                let f = instance.get_func(&self.store, "main").unwrap();
                func = f.typed::<(), ()>(&self.store).unwrap();
            },
            Err(_) => panic!("lock failed")
        }
        // do not lock the lines below: wasm calls host functions which may call back to wasmi containing lock
        let call = func.call_resumable(&mut self.store, ()).unwrap();
        match call {
            TypedResumableCall::Finished(_) => {}
            TypedResumableCall::Resumable(invocation) => {
                let host_err = invocation.host_error();
                return Ok(format!("error:{}", host_err.i32_exit_status().unwrap()))
            }
        }
        let json_body = match self.lock.lock() {
            Ok(_) => {
                self.store.tracer.to_json()
            },
            Err(_) => panic!("lock failed")
        };
        Ok(json_body)
    }

    pub fn memory_data(&mut self) -> Vec<u8> {
        self.fetch_memory_data(&self.instance.unwrap())
    }

    fn fetch_memory_data_no_lock(&self, instance: &Instance) -> Vec<u8> {
        let mut memory_data = Vec::<u8>::new();
        for export in instance.exports(&self.store) {
            if export.name() != "memory" { continue }
            if let ExternType::Memory(_) = export.ty(&self.store) {
                let export = instance.get_export(&self.store, export.name());
                if let Some(export) = export {
                    if let Some(memory) = export.into_memory() {
                        let mem_data = memory.data(&self.store);
                        memory_data = mem_data.into();
                    }
                }
            }
        }
        memory_data
    }

    // fn get_memory_data_ptr(&mut self) -> *mut u8 {
    //     self.memory_data_ptr
    // }

    fn fetch_memory_data(&self, instance: &Instance) -> Vec<u8> {
        match self.lock.lock() {
            Ok(_) => {
                self.fetch_memory_data_no_lock(instance)
            }
            Err(_) => panic!("lock failed")
        }
    }

    pub fn trace_memory_change(&mut self, offset: u32, len: u32, data: &[u8]) {
        match self.lock.lock() {
            Ok(_) => { self.store.tracer.memory_change(offset, len, data); }
            Err(_) => panic!("lock failed")
        }
    }

    pub fn add_host_fn_cb<Params, Results>(&mut self, name: String, func: impl IntoFunc<(), Params, Results>) -> Result<(), String> {
        match self.lock.lock() {
            Ok(_) => {
                if self.host_fns.contains_key(name.as_str()) {
                    return Err(format!("there is already fn with name: {}", &name));
                };
                let host_fn = Func::wrap_with_meta(&mut self.store, func, name.clone());
                self.host_fns.insert(name, host_fn);
            }
            Err(_) => panic!("lock failed")
        }
        Ok(())
    }

    pub fn register_cb_on_after_item_added_to_logs(&mut self, cb: Box<dyn Fn(OpCodeState)>) {
        self.store.tracer.set_cb_on_after_item_added_to_logs(cb)
    }
}