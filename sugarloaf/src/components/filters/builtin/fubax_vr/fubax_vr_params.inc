layout(std140, set = 0, binding = 0) uniform UBO
{
   mat4 MVP;
   float SoloLines, TestGrid, IPD, StereoSwitch, FOV, LensType, K1, K2,
      K3, K4, P1, P2, P3, ImageScale, PerspectiveSwitch, ChGreenK1, ChGreenK2,
      ChGreenK3, ChGreenK4, SoloGreen, ChBlueK1, ChBlueK2, ChBlueK3, ChBlueK4,
      SoloBlue, ChromaticAbrSwitch, ChGreenOffsetL_x, ChGreenOffsetL_y,
      ChBlueOffsetL_x, ChBlueOffsetL_y, ChGreenOffsetR_x, ChGreenOffsetR_y,
      ChBlueOffsetR_x, ChBlueOffsetR_y, Strength, Clamp, Offset, Preview,
      Sharpen, Nose, Brightness, Scale_x, Scale_y, imgzoom, aspect_x, aspect_y;
      /*ParallaxOffset, ParallaxCenter, ParallaxSteps, ParallaxMaskScalar,
      ParallaxSwitch */
} global;

#ifndef MaximumParallaxSteps
#define MaximumParallaxSteps 1024 // Defefine max steps to make loop finite
#endif

// Grid settings
#ifndef BoxAmount
#define BoxAmount 31 // Number of boxes horizontally (choose odd number)
#endif

#ifndef thicknessA
#define thicknessA 0.25 // White grid thickness
#endif

#ifndef thicknessB
#define thicknessB 0.125 // Yellow cross thickness
#endif

#ifndef crossColor
#define crossColor vec3(1.0, 1.0, 0.0) // Center cross color (yellow)
#endif

#pragma parameter TestGrid "Toggle Calibration Grid" 0.0 0.0 1.0 1.0
#pragma parameter StereoSwitch "Toggle Stereo Vision" 1.0 0.0 1.0 1.0
#pragma parameter PerspectiveSwitch "Toggle Lens Correction" 1.0 0.0 1.0 1.0
//#pragma parameter ParallaxSwitch "Toggle Parallax Effect" 1.0 0.0 1.0 1.0
#pragma parameter ChromaticAbrSwitch "Toggle Chromatic Correction" 1.0 0.0 1.0 1.0
#pragma parameter Nose "Toggle Virtual Nose (helps nausea)" 0.0 0.0 1.0 1.0
#pragma parameter Sharpen "Toggle Sharpening" 1.0 0.0 1.0 1.0
#pragma parameter SoloLines "Lines (All, Horz, Vert, Rad)" 0.0 0.0 3.0 1.0
#pragma parameter IPD "IPD (Interpupillary Distance)" 0.500 0.0 0.75 0.001
#pragma parameter imgzoom "Image Zoom" 1.6 0.1 5.0 0.01
#pragma parameter aspect_x "Image Aspect Ratio Numerator" 64.0 1.0 256.0 1.0
#pragma parameter aspect_y "Image Aspect Ratio Denominator" 49.0 1.0 256. 1.0
//#pragma parameter ParallaxOffset "Parallax Horizontal Offset" 0.355 0.0 1.0 0.001
//#pragma parameter ParallaxCenter "Parallax Rotation Center (ZPD)" 1.0 0.0 1.0 0.001
//#pragma parameter ParallaxSteps "Parallax Sampling Steps" 16.0 1.0 128.0 0.2
//#pragma parameter ParallaxMaskScalar "Parallax Gap Compensation" 10.0 0.0 32.0 0.2
#pragma parameter FOV "Lens Distortion Power" 96.0 0.0 170.0 0.1
#pragma parameter LensType "Lens Dist Type (Ortho, Equisolid, Equidist, Stereo)" 0.0 0.0 3.0 1.0
#pragma parameter K1 "Radial Correction #1 (Brown-Conrady Model)" 0.0 0.0 1.0 0.01
#pragma parameter K2 "Radial Correction #2 (Brown-Conrady Model)" 0.0 0.0 1.0 0.01
#pragma parameter K3 "Radial Correction #3 (Brown-Conrady Model)" 0.0 0.0 1.0 0.01
#pragma parameter K4 "Radial Correction #4 (Brown-Conrady Model)" 0.0 0.0 1.0 0.01
#pragma parameter P1 "Tangential Correction #1 (Brown-Conrady Model)" 0.0 0.0 1.0 0.001
#pragma parameter P2 "Tangential Correction #2 (Brown-Conrady Model)" 0.0 0.0 1.0 0.001
#pragma parameter P3 "Tangential Correction #3 (Brown-Conrady Model)" 0.0 0.0 1.0 0.001
#pragma parameter ImageScale "Image Scale" 1.0 0.25 1.0 0.01
#pragma parameter ChGreenK1 "Chromatic Green Correction (Zoom)" 0.0 0.0 1.0 0.001 
#pragma parameter ChGreenK2 "Chromatic Green Correction (K1)" 0.0 0.0 1.0 0.001
#pragma parameter ChGreenK3 "Chromatic Green Correction (K2)" 0.0 0.0 1.0 0.001
#pragma parameter ChGreenK4 "Chromatic Green Correction (K3)" 0.0 0.0 1.0 0.001
#pragma parameter SoloGreen "Green Channel Adj Solo" 0.0 0.0 1.0 1.0
#pragma parameter ChBlueK1 "Chromatic Blue Correction (Zoom)" 0.0 0.0 1.0 0.001
#pragma parameter ChBlueK2 "Chromatic Blue Correction (K1)" 0.0 0.0 1.0 0.001
#pragma parameter ChBlueK3 "Chromatic Blue Correction (K2)" 0.0 0.0 1.0 0.001
#pragma parameter ChBlueK4 "Chromatic Blue Correction (K3)" 0.0 0.0 1.0 0.001
#pragma parameter SoloBlue "Blue Channel Adj Solo" 0.0 0.0 1.0 1.0
#pragma parameter ChGreenOffsetL_x "Left Green Center Offset X" 0.0 -0.2 0.2 0.001
#pragma parameter ChGreenOffsetL_y "Left Green Center Offset Y" 0.0 -0.2 0.2 0.001
#pragma parameter ChBlueOffsetL_x "Left Blue Center Offset X" 0.0 -0.2 0.2 0.001
#pragma parameter ChBlueOffsetL_y "Left Blue Center Offset Y" 0.0 -0.2 0.2 0.001
#pragma parameter ChGreenOffsetR_x "Right Green Center Offset X" 0.0 -0.2 0.2 0.001
#pragma parameter ChGreenOffsetR_y "Right Green Center Offset Y" 0.0 -0.2 0.2 0.001
#pragma parameter ChBlueOffsetR_x "Right Blue Center Offset X" 0.0 -0.2 0.2 0.001
#pragma parameter ChBlueOffsetR_y "Right Blue Center Offset Y" 0.0 -0.2 0.2 0.001
#pragma parameter Strength "Sharpen Strength" 25.0 0.0 100.0 0.5
#pragma parameter Clamp "Sharpen Clamping" 0.65 0.5 1.0 0.001
#pragma parameter Offset "High-pass Offset" 0.1 0.01 2.0 0.002
#pragma parameter Preview "Preview Sharpen Layer" 0.0 0.0 1.0 1.0
#pragma parameter Brightness "Virtual Nose Brightness" 1.0 0.0 1.0 0.01
#pragma parameter Scale_x "Virtual Nose Scale X" 0.382 0.1 1.0 0.01
#pragma parameter Scale_y "Virtual Nose Scale Y" 0.618 0.1 1.0 0.01

int SoloLines = int(global.SoloLines);
bool TestGrid = bool(global.TestGrid);
float IPD = global.IPD;
bool StereoSwitch = bool(global.StereoSwitch);
//float ParallaxOffset = global.ParallaxOffset;
//float ParallaxCenter = global.ParallaxCenter;
//int ParallaxSteps = int(global.ParallaxSteps);
//int ParallaxMaskScalar = int(global.ParallaxMaskScalar);
//bool ParallaxSwitch = bool(global.ParallaxSwitch);
int FOV = int(global.FOV);
int LensType = int(global.LensType);
vec4 K = vec4(global.K1, global.K2, global.K3, global.K4);
vec3 P = vec3(global.P1, global.P2, global.P3);
float ImageScale = global.ImageScale;
bool PerspectiveSwitch = bool(global.PerspectiveSwitch);
vec4 ChGreenK = vec4(global.ChGreenK1, global.ChGreenK2, global.ChGreenK3,
   global.ChGreenK4);
bool SoloGreen = bool(global.SoloGreen);
vec4 ChBlueK = vec4(global.ChBlueK1, global.ChBlueK2, global.ChBlueK3,
   global.ChBlueK4);
bool SoloBlue = bool(global.SoloBlue);
bool ChromaticAbrSwitch = bool(global.ChromaticAbrSwitch);
vec2 ChGreenOffsetL = vec2(global.ChGreenOffsetL_x, global.ChGreenOffsetL_y);
vec2 ChBlueOffsetL = vec2(global.ChBlueOffsetL_x, global.ChBlueOffsetL_y);
vec2 ChGreenOffsetR = vec2(global.ChGreenOffsetR_x, global.ChGreenOffsetR_y);
vec2 ChBlueOffsetR = vec2(global.ChBlueOffsetR_x, global.ChBlueOffsetR_y);
float Strength = global.Strength;
float Clamp = global.Clamp;
float Offset = global.Offset;
bool Preview = bool(global.Preview);
bool Sharpen = bool(global.Sharpen);
bool Nose = bool(global.Nose);
float Brightness = global.Brightness;
vec2 Scale = vec2(global.Scale_x, global.Scale_y);
float imgzoom = global.imgzoom;
vec2 img_ar = vec2(global.aspect_x, global.aspect_y);
