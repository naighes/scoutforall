# Models

`seeta_fd_frontal_v1.0.bin` (1.2 MB): the frontal face detector of the
SeetaFace engine (funnel-structured cascade), BSD 2-Clause license, taken
from the `rustface` repository (https://github.com/atomashpolskiy/rustface).
It runs locally, in `core::faces`, on every platform (pure Rust, no native
library). Nothing leaves the device and nobody is recognized: it only finds
where the face is.

`u2netp.onnx` (4.6 MB): the U2NetP foreground segmentation model distributed
with rembg v0.0.0 (MIT; https://github.com/danielgatis/rembg). It runs locally
through `tract-onnx`, with no native library. `core::faces` uses its mask to
make the person photo transparent before the PNG is saved.
