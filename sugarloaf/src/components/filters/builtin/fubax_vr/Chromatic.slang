#version 450

/*
/// VR shader ///

Make any game VR and any screen with lenses a VR headset.
Thanks to this shader you'll be able to correct distortions of any lens types
(DIY, experimental) and chromatic aberration.
Also if a game outputs depth pass you can have a stereo-3D vision thanks to
the parallax mapping (which needs some further improvement).

Copyright (c) 2019 Jacob Max Fober

This work is licensed under the Creative Commons 
Attribution-NonCommercial-ShareAlike 4.0 International License.
To view a copy of this license, visit 
http://creativecommons.org/licenses/by-nc-sa/4.0/

If you want to use it commercially, contact me at jakub.m.fober@pm.me
If you have questions, visit https://reshade.me/forum/shader-discussion/

I'm author of most of equations present here,
beside Brown-Conrady distortion correction model and
Parallax Steep and Occlusion mapping which
I changed and adopted from various sources.

Version 0.4.2 alpha
*/

layout(push_constant) uniform Push
{
	vec4 SourceSize;
	vec4 OriginalSize;
	vec4 OutputSize;
	uint FrameCount;
} params;

#include "fubax_vr_params.inc"

#pragma stage vertex
layout(location = 0) in vec4 Position;
layout(location = 1) in vec2 TexCoord;
layout(location = 0) out vec2 texcoord;

void main()
{
   gl_Position = global.MVP * Position;
   texcoord = TexCoord;
}

#pragma stage fragment
layout(location = 0) in vec2 texcoord;
layout(location = 0) out vec4 FragColor;
layout(set = 0, binding = 2) uniform sampler2D Source;

#include "fubax_vr_shared_funcs.inc"

void main()
{
	// Bypass chromatic aberration switch
	if(!ChromaticAbrSwitch)
	{ 
	   FragColor = vec4(texture(Source, texcoord).rgb, 1.0);
	   return;
	}

	// Get display aspect ratio (horizontal/vertical resolution)
	float rAspect = params.OutputSize.x*params.OutputSize.w;

	// Generate negative-positive stereo mask
	float SideScreenSwitch = step(0.5, texcoord.x)*2.0-1.0;

	// Divide screen in two if stereo vision mode enabled
	vec2 CenterCoord = StereoSwitch? StereoVision(texcoord, IPD) : texcoord;

	CenterCoord = CenterCoord*2.0-1.0; // Center coordinates
	CenterCoord.x *= rAspect; // Correct aspect ratio

	float Diagonal = rAspect;
	Diagonal *= StereoSwitch ? 0.5 : 1.0;
	Diagonal = length(vec2(Diagonal, 1.0));

	CenterCoord /= Diagonal; // Normalize diagonally

	// Left/right eye mask
	float L = step(0.5, 1.0-texcoord.x);
	float R = step(0.5, texcoord.x);

	// Offset center green
	vec2 CoordGreen = ChGreenOffsetL * L + ChGreenOffsetR * R;
	CoordGreen.x *= -1.0;
	CoordGreen = 0.01 * CoordGreen + CenterCoord;
	// Offset center blue
	vec2 CoordBlue = ChBlueOffsetL * L + ChBlueOffsetR * R;
	CoordBlue.x *= -1.0;
	CoordBlue = 0.01 * CoordBlue + CenterCoord;

//	float RadiusGreen = dot(CoordGreen, CoordGreen); // Radius squared (techically accurate)
//	float RadiusBlue = dot(CoordBlue, CoordBlue); // Radius squared (techically accurate)
	float RadiusGreen = length(CoordGreen); // Radius
	float RadiusBlue = length(CoordBlue); // Radius

	// Calculate radial distortion K
	float correctionGreenK = (1.0+ChGreenK.x)*kRadial(RadiusGreen, ChGreenK.y, ChGreenK.z, ChGreenK.w, 0.0);
	float correctionBlueK = (1.0+ChBlueK.x)*kRadial(RadiusBlue, ChBlueK.y, ChBlueK.z, ChBlueK.w, 0.0);

	// Apply chromatic aberration correction
	CoordGreen = CoordGreen * correctionGreenK;
	CoordBlue = CoordBlue * correctionBlueK;

	CoordGreen *= Diagonal; CoordBlue *= Diagonal; // Back to vertical normalization
	CoordGreen.x /= rAspect; CoordBlue.x /= rAspect; // Back to square

	// Move origin to left top corner
	CoordGreen = CoordGreen * 0.5 + 0.5; CoordBlue = CoordBlue * 0.5 + 0.5;

	// Generate border mask for green and blue channel
	float MaskBlue, MaskGreen; if(StereoSwitch)
	{
		// Mask compensation for center cut
		float CenterCut = 0.5+(0.5-IPD)*SideScreenSwitch;

		// Mask sides and center cut for blue channel
		vec2 MaskCoordBlue;
		MaskCoordBlue.x = CoordBlue.x*2.0 - CenterCut; // Compensate for 2 views
		MaskCoordBlue.y = CoordBlue.y;
		MaskBlue = BorderMaskAA(MaskCoordBlue);

		// Mask sides and center cut for green channel
		vec2 MaskCoordGreen;
		MaskCoordGreen.x = CoordGreen.x*2.0 - CenterCut; // Compensate for 2 views
		MaskCoordGreen.y = CoordGreen.y;
		MaskGreen = BorderMaskAA(MaskCoordGreen);

		// Reverse stereo coordinates to single view
		CoordGreen = InvStereoVision(CoordGreen, int(SideScreenSwitch), IPD);
		CoordBlue = InvStereoVision(CoordBlue, int(SideScreenSwitch), IPD);
	}
	else
	{
		MaskBlue = BorderMaskAA(CoordBlue);
		MaskGreen = BorderMaskAA(CoordGreen);
	};

	vec3 Image;
	// Sample image red
	Image.r = texture(Source, texcoord).r;
	// Sample image green
	Image.g = mix(
		texture(Source, CoordGreen).g,
		0.0, // Black borders
		MaskGreen // Anti-aliased border mask
	);
	// Sample image blue
	Image.b = mix(
		texture(Source, CoordBlue).b,
		0.0, // Black borders
		MaskBlue // Anti-aliased border mask
	);

	// Display chromatic aberration
   FragColor = vec4(Image, 1.0);
}
