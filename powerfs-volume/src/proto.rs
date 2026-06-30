pub mod powerfs {
    tonic::include_proto!("powerfs");
}

pub use powerfs::volume_service_server::{VolumeService, VolumeServiceServer};
pub use powerfs::{
    CreateVolumeRequest,
    CreateVolumeResponse,
    DeleteVolumeRequest,
    DeleteVolumeResponse,
    WriteNeedleRequest,
    WriteNeedleResponse,
    ReadNeedleRequest,
    ReadNeedleResponse,
    DeleteNeedleRequest,
    DeleteNeedleResponse,
    ListVolumesRequest,
    ListVolumesResponse,
    GetNodeInfoRequest,
    GetNodeInfoResponse,
    VolumeInfo,
};
