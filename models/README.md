# Models

Blurlist runs two neural networks locally with [ONNX Runtime](https://onnxruntime.ai):
a **face detector** that finds faces and five landmarks, and a **face recogniser**
that turns an aligned face into an identity vector.

`node scripts/fetch-models.mjs` downloads the bundled models listed in
[`manifest.json`](manifest.json) and checks their SHA-256 hashes. The `.onnx`
files are not committed to git.

## Bundled (permissive licences)

| Model | Role | Licence | Source |
| --- | --- | --- | --- |
| YuNet 2023mar | Detector (5 landmarks) | [MIT](licenses/YuNet-MIT.txt) | [OpenCV Zoo](https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet) |
| SFace 2021dec | Recogniser, 128-d, margin-based (ArcFace-class) | [Apache-2.0](licenses/SFace-Apache-2.0.txt) | [OpenCV Zoo](https://github.com/opencv/opencv_zoo/tree/main/models/face_recognition_sface) |

## Supported, never bundled (research-only weights)

Blurlist also implements InsightFace's **SCRFD** detector and **ArcFace**
recognisers. The code is MIT-licensed, but InsightFace states that its pretrained
weights are *"available for non-commercial research purposes only"*. Schools,
camps and nonprofits running Blurlist in production are not doing research, so
these files are never shipped or downloaded by Blurlist.

If you are entitled to use them, place any of these files in the app's models
folder (Settings → Models → *Open models folder*) and select them in Settings:

| File | Model | From |
| --- | --- | --- |
| `det_10g.onnx` | SCRFD-10G detector | InsightFace `buffalo_l` |
| `det_500m.onnx` | SCRFD-500M detector | InsightFace `buffalo_sc` |
| `w600k_r50.onnx` | ArcFace ResNet-50 (WebFace600K) | InsightFace `buffalo_l` |
| `w600k_mbf.onnx` | ArcFace MobileFaceNet (WebFace600K) | InsightFace `buffalo_sc` |

Any other ONNX model with the same input/output layout as one of the four
families (YuNet, SCRFD, SFace, ArcFace) can be imported from Settings → Models →
*Import model…*, for example an Apache-2.0 ArcFace-class recogniser. Always check
the model card's licence before using it.
