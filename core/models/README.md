# Models

`seeta_fd_frontal_v1.0.bin` (1.2 MB): the frontal face detector of the
SeetaFace engine (funnel-structured cascade), BSD 2-Clause license (copy in
`seeta_fd_frontal_v1.0.LICENSE`), taken from the `rustface` repository
(https://github.com/atomashpolskiy/rustface).
It runs locally, in `core::faces`, on every platform (pure Rust, no native
library). Nothing leaves the device and nobody is recognized: it only finds
where the face is.

`u2netp.onnx` (4.6 MB): the U2NetP foreground segmentation model, the weights
of U-2-Net (https://github.com/xuebinqin/U-2-Net, Apache License 2.0, copy in
`u2netp.LICENSE`) converted to ONNX and distributed by rembg
(https://github.com/danielgatis/rembg, release v0.0.0, md5
`8e83ca70e441ab06c318d82300c84806`). The authors publish no separate terms for
the weights; they were trained on the DUTS-TR dataset. It runs locally through
`tract-onnx`, with no native library. `core::faces` uses its mask to make the
person photo transparent before the PNG is saved.

The texts of both licenses, with the one of the fonts of the reports, are
shown by the apps in the settings (`core::licenses`), as they ask.
