//! Isolated native-engine execution over length-prefixed, versioned stdio.
//!
//! Every process has its own address space, allowing source engines with
//! process-global state (or non-Send scene/renderer objects) to execute without
//! placing their internals inside the universal OASIS host. The protocol
//! transports original typed state and forwards WorldPort calls synchronously.
//! No engine names or game-pair-specific conversions are permitted here.

use std::cell::RefCell;
use std::io::{self, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;

use oasis_contracts::{
    ClockStep, ContractError, ContractResult, EntityView, FrameMap, GeometryRequest,
    GeometryResult, Id, InputIntent, InteractionRequest, ModuleDescriptor, NativeHandle,
    NativeModule, Snapshot, SpatialHit, SpatialQuery, StepOutput, WorldPort,
};
use serde::{Deserialize, Serialize};

const PROTOCOL: u32 = 1;
const MAX_FRAME: usize = 16 * 1024 * 1024;

fn internal(error: impl std::fmt::Display) -> ContractError {
    ContractError::Internal(format!("native worker channel failure: {error}"))
}

#[derive(Serialize, Deserialize)]
enum Call {
    Instantiate { entity: EntityView, saved: Option<Snapshot> },
    Step { clock: ClockStep, inputs: Vec<InputIntent> },
    Snapshot(NativeHandle),
    Restore { handle: NativeHandle, snapshot: Snapshot },
    Remove(NativeHandle),
    Shutdown,
}
#[derive(Serialize, Deserialize)]
enum Reply {
    Handle(NativeHandle),
    Stepped(StepOutput),
    Snapshot(Snapshot),
    Done,
}
#[derive(Serialize, Deserialize)]
enum WorldCall {
    Geometry(GeometryRequest),
    Query(SpatialQuery),
    FrameMap { source: Id, destination: Id },
    EntityView(Id),
    Submit(InteractionRequest),
}
#[derive(Serialize, Deserialize)]
enum WorldReply {
    Geometry(GeometryResult),
    Query(Vec<SpatialHit>),
    FrameMap(FrameMap),
    EntityView(EntityView),
    Done,
}
#[derive(Serialize, Deserialize)]
enum Frame {
    Ready { protocol: u32, descriptor: ModuleDescriptor },
    Call(Call),
    Return(ContractResult<Reply>),
    WorldCall(WorldCall),
    WorldReturn(ContractResult<WorldReply>),
}

fn write_frame(writer: &mut impl Write, frame: &Frame) -> io::Result<()> {
    let bytes=serde_json::to_vec(frame)
        .map_err(|error|io::Error::new(io::ErrorKind::InvalidData,error))?;
    if bytes.len()>MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData,"oversized native frame"));
    }
    writer.write_all(&(bytes.len() as u32).to_be_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()
}

fn read_frame(reader: &mut impl Read) -> io::Result<Frame> {
    let mut header=[0u8;4];
    reader.read_exact(&mut header)?;
    let len=u32::from_be_bytes(header) as usize;
    if len==0 || len>MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData,"invalid native frame size"));
    }
    let mut bytes=vec![0u8;len];
    reader.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes)
        .map_err(|error|io::Error::new(io::ErrorKind::InvalidData,error))
}

/// Native engine implementation living only in its OWN worker process.
///
/// Unlike `NativeModule`, this trait deliberately has no `Send` bound:
/// original game engines can own thread-affine/!Send renderer and scene state.
/// Existing Send-native modules automatically satisfy this trait.
pub trait LocalNativeModule {
    fn descriptor(&self) -> ModuleDescriptor;
    fn instantiate(&mut self, entity: &EntityView, saved: Option<&Snapshot>)
        -> ContractResult<NativeHandle>;
    fn step(&mut self, clock: ClockStep, inputs: &[InputIntent],
            world: &mut dyn WorldPort) -> ContractResult<StepOutput>;
    fn snapshot(&self, handle: NativeHandle) -> ContractResult<Snapshot>;
    fn restore(&mut self, handle: NativeHandle, snapshot: &Snapshot)
        -> ContractResult<()>;
    fn remove(&mut self, handle: NativeHandle) -> ContractResult<()>;
}

impl<T: NativeModule> LocalNativeModule for T {
    fn descriptor(&self) -> ModuleDescriptor { NativeModule::descriptor(self) }
    fn instantiate(&mut self, entity: &EntityView, saved: Option<&Snapshot>)
        -> ContractResult<NativeHandle> { NativeModule::instantiate(self,entity,saved) }
    fn step(&mut self,clock:ClockStep,inputs:&[InputIntent],world:&mut dyn WorldPort)
        -> ContractResult<StepOutput> { NativeModule::step(self,clock,inputs,world) }
    fn snapshot(&self,handle:NativeHandle)->ContractResult<Snapshot>{
        NativeModule::snapshot(self,handle)
    }
    fn restore(&mut self,handle:NativeHandle,snapshot:&Snapshot)->ContractResult<()>{
        NativeModule::restore(self,handle,snapshot)
    }
    fn remove(&mut self,handle:NativeHandle)->ContractResult<()>{
        NativeModule::remove(self,handle)
    }
}

/// Uses the original WorldPort callback rather than substituting a fake
/// environment in the game-worker process. Calls are synchronous and cannot
/// escape server-side lease/scheduling decisions.
struct WorkerWorld<'a,R:Read,W:Write> {
    io: RefCell<(&'a mut R,&'a mut W)>,
}
impl<R:Read,W:Write> WorkerWorld<'_,R,W> {
    fn exchange(&self,request:WorldCall)->ContractResult<WorldReply>{
        let mut pair=self.io.borrow_mut();
        write_frame(&mut *pair.1,&Frame::WorldCall(request)).map_err(internal)?;
        match read_frame(&mut *pair.0).map_err(internal)? {
            Frame::WorldReturn(result)=>result,
            _=>Err(internal("unexpected reply to WorldPort callback")),
        }
    }
}
impl<R:Read,W:Write> WorldPort for WorkerWorld<'_,R,W> {
    fn geometry(&mut self,request:GeometryRequest)->ContractResult<GeometryResult>{
        match self.exchange(WorldCall::Geometry(request))? {
            WorldReply::Geometry(data)=>Ok(data),
            _=>Err(internal("wrong geometry reply")),
        }
    }
    fn query(&mut self,request:SpatialQuery)->ContractResult<Vec<SpatialHit>>{
        match self.exchange(WorldCall::Query(request))? {
            WorldReply::Query(data)=>Ok(data),
            _=>Err(internal("wrong spatial query reply")),
        }
    }
    fn frame_map(&self,source:Id,destination:Id)->ContractResult<FrameMap>{
        match self.exchange(WorldCall::FrameMap{source,destination})? {
            WorldReply::FrameMap(data)=>Ok(data),
            _=>Err(internal("wrong spatial frame reply")),
        }
    }
    fn entity_view(&self,id:Id)->ContractResult<EntityView>{
        match self.exchange(WorldCall::EntityView(id))? {
            WorldReply::EntityView(data)=>Ok(data),
            _=>Err(internal("wrong entity view reply")),
        }
    }
    fn submit_interaction(&mut self,request:InteractionRequest)->ContractResult<()>{
        match self.exchange(WorldCall::Submit(request))? {
            WorldReply::Done=>Ok(()),
            _=>Err(internal("wrong interaction acknowledgement")),
        }
    }
}

/// Call from the game-specific worker binary's main thread. A `!Send`
/// engine stays isolated and may invoke real WorldPort methods while stepping.
/// Its stdout is dedicated to this protocol; diagnostics must use stderr.
pub fn serve_worker<M:LocalNativeModule>(mut module:M)->io::Result<()> {
    let input=io::stdin();
    let output=io::stdout();
    let mut reader=BufReader::new(input.lock());
    let mut writer=output.lock();
    write_frame(&mut writer,&Frame::Ready{
        protocol:PROTOCOL,descriptor:module.descriptor(),
    })?;
    loop {
        let call=match read_frame(&mut reader)?{
            Frame::Call(call)=>call,
            _=>return Err(io::Error::new(io::ErrorKind::InvalidData,
                "unexpected native command")),
        };
        if matches!(call,Call::Shutdown){return Ok(());}
        let reply=match call {
            Call::Instantiate{entity,saved}=>module.instantiate(&entity,saved.as_ref())
                .map(Reply::Handle),
            Call::Step{clock,inputs}=>{
                let mut world=WorkerWorld{
                    io:RefCell::new((&mut reader,&mut writer)),
                };
                module.step(clock,&inputs,&mut world).map(Reply::Stepped)
            },
            Call::Snapshot(handle)=>module.snapshot(handle).map(Reply::Snapshot),
            Call::Restore{handle,snapshot}=>module.restore(handle,&snapshot)
                .map(|()|Reply::Done),
            Call::Remove(handle)=>module.remove(handle).map(|()|Reply::Done),
            Call::Shutdown=>unreachable!(),
        };
        write_frame(&mut writer,&Frame::Return(reply))?;
    }
}

/// Platform-side proxy. Rust's `Child` and owned pipes are `Send`, although
/// the original game state in the worker need not be. One worker per isolated
/// engine context; no in-process game-global state is shared across contexts.
struct WorkerIo {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}
pub struct ProcessModule {
    child: Child,
    io: Mutex<WorkerIo>,
    descriptor: ModuleDescriptor,
    context_id: Id,
}

impl ProcessModule {
    pub fn spawn(command: &mut Command, module_id: Id, context_id: Id)
        ->ContractResult<Self>{
        let mut child=command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn().map_err(internal)?;
        let stdin=child.stdin.take().ok_or_else(||internal("stdin unavailable"))?;
        let stdout=child.stdout.take().ok_or_else(||internal("stdout unavailable"))?;
        let mut stdout=BufReader::new(stdout);
        let descriptor=match read_frame(&mut stdout).map_err(internal)? {
            Frame::Ready{protocol,descriptor}
                if protocol==PROTOCOL
                    && descriptor.module_id==module_id
                    && descriptor.contract_major==0
                    && descriptor.contract_minor>=1=>descriptor,
            _=>{
                let _=child.kill();
                let _=child.wait();
                return Err(internal("untrusted or incompatible native worker handshake"));
            },
        };
        Ok(Self{
            child,
            io:Mutex::new(WorkerIo{stdin,stdout}),
            descriptor,context_id,
        })
    }

    fn exchange(&self,command:Call,mut world:Option<&mut dyn WorldPort>)
        ->ContractResult<Reply>{
        let mut io=self.io.lock().map_err(internal)?;
        write_frame(&mut io.stdin,&Frame::Call(command)).map_err(internal)?;
        loop {
            match read_frame(&mut io.stdout).map_err(internal)? {
                Frame::Return(result)=>return result,
                Frame::WorldCall(request)=>{
                    let handler=world.as_deref_mut()
                        .ok_or_else(||internal("unexpected world callback outside step"))?;
                    let response=match request{
                        WorldCall::Geometry(r)=>handler.geometry(r).map(WorldReply::Geometry),
                        WorldCall::Query(r)=>handler.query(r).map(WorldReply::Query),
                        WorldCall::FrameMap{source,destination}=>
                            handler.frame_map(source,destination).map(WorldReply::FrameMap),
                        WorldCall::EntityView(id)=>handler.entity_view(id)
                            .map(WorldReply::EntityView),
                        WorldCall::Submit(request)=>handler.submit_interaction(request)
                            .map(|()|WorldReply::Done),
                    };
                    write_frame(&mut io.stdin,&Frame::WorldReturn(response))
                        .map_err(internal)?;
                },
                _=>return Err(internal("unexpected frame from game process")),
            }
        }
    }
}

impl NativeModule for ProcessModule {
    fn descriptor(&self)->ModuleDescriptor{self.descriptor.clone()}
    fn instantiate(&mut self,entity:&EntityView,saved:Option<&Snapshot>)
        ->ContractResult<NativeHandle>{
        match self.exchange(Call::Instantiate{
            entity:entity.clone(),saved:saved.cloned(),
        },None)? {
            Reply::Handle(handle) if handle.context_id==self.context_id=>Ok(handle),
            _=>Err(internal("worker returned wrong context or handle")),
        }
    }
    fn step(&mut self,clock:ClockStep,inputs:&[InputIntent],world:&mut dyn WorldPort)
        ->ContractResult<StepOutput>{
        match self.exchange(Call::Step{
            clock,inputs:inputs.to_vec(),
        },Some(world))?{
            Reply::Stepped(output)=>Ok(output),
            _=>Err(internal("wrong native step reply")),
        }
    }
    fn snapshot(&self,handle:NativeHandle)->ContractResult<Snapshot>{
        match self.exchange(Call::Snapshot(handle),None)?{
            Reply::Snapshot(snapshot)=>Ok(snapshot),
            _=>Err(internal("wrong native checkpoint reply")),
        }
    }
    fn restore(&mut self,handle:NativeHandle,snapshot:&Snapshot)->ContractResult<()>{
        match self.exchange(Call::Restore{
            handle,snapshot:snapshot.clone(),
        },None)? {
            Reply::Done=>Ok(()),
            _=>Err(internal("wrong native restore reply")),
        }
    }
    fn remove(&mut self,handle:NativeHandle)->ContractResult<()>{
        match self.exchange(Call::Remove(handle),None)?{
            Reply::Done=>Ok(()),
            _=>Err(internal("wrong native remove reply")),
        }
    }
}

impl Drop for ProcessModule {
    fn drop(&mut self){
        // Kill even if the child is deadlocked during a native tick. The
        // durable authority/lease layer is responsible for fencing restarts.
        let _=self.child.kill();
        let _=self.child.wait();
    }
}
