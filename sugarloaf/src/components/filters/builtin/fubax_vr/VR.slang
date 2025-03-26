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
   texcoord = TexCoord * 1.00001;
}

#pragma stage fragment
layout(location = 0) in vec2 texcoord;
layout(location = 0) out vec4 FragColor;
layout(set = 0, binding = 2) uniform sampler2D Source;

#include "fubax_vr_shared_funcs.inc"

void main()
{
	// Get display aspect ratio (horizontal/vertical resolution)
	const float rAspect = params.OutputSize.x*params.OutputSize.w;

	// Divide screen in two
	vec2 UvCoord = StereoSwitch? StereoVision(texcoord, IPD) : texcoord;

	// Generate negative-positive stereo mask
	float StereoMask = step(0.5, texcoord.x)*2.0-1.0;

	// Correct lens distortion
	if(PerspectiveSwitch)
	{
		// Center coordinates
		UvCoord = UvCoord*2.0-1.0;
		UvCoord.x *= rAspect;
		vec2 StereoCoord = UvCoord; // Save coordinates for Brown-Conrady correction

		// Base distortion correction
		if(bool(FOV)) // If FOV is not equal 0
		{
			float radFOV = radians(FOV);
			// Calculate radius
			float Radius = length(UvCoord);
			// Apply base lens correction
			switch(LensType)
			{
				case 0:
					{ UvCoord *= Orthographic(radFOV, Radius); break; }
				case 1:
					{ UvCoord *= Equisolid(radFOV, Radius); break; }
				case 2:
					{ UvCoord *= Equidistant(radFOV, Radius); break; }
				case 3:
					{ UvCoord *= Stereographic(radFOV, Radius); break; }
			}
		};

		// Lens geometric aberration correction (Brown-Conrady model)
		float Diagonal = rAspect;
		Diagonal *= StereoSwitch ? 0.5 : 1.0;
		Diagonal = length(vec2(Diagonal, 1.0));
		float InvDiagonal2 = 1.0 / pow(Diagonal, 2);

		StereoCoord /= Diagonal; // Normalize diagonally
		float Radius2 = dot(StereoCoord, StereoCoord); // Squared radius
		float correctionK = kRadial(Radius2, K.x, K.y, K.z, K.w);
		// Apply negative-positive stereo mask for tangental distortion (flip side)
		float SideScreenSwitch = (StereoSwitch) ? StereoMask : 1.0;

		vec2 correctionP = pTangental(
			StereoCoord,
			Radius2,
			P.x * SideScreenSwitch,
			P.y,
			P.z,
			0.0
		);
		// Expand background to vertical border (but not for test grid for ease of calibration)
		UvCoord /= TestGrid ? vec2(1.0) : vec2(kRadial(InvDiagonal2, K.x, K.y, K.z, K.w));
		UvCoord = UvCoord * correctionK + correctionP; // Apply lens correction

		// Scale image
		UvCoord /= TestGrid ? vec2(1.0) : vec2(ImageScale);

		// Revert aspect ratio to square
		UvCoord.x /= rAspect;
		// Move origin back to left top corner
		UvCoord = UvCoord*0.5 + vec2(0.5);
	}

	// Display test grid
	if(TestGrid) {
	   FragColor = vec4(Grid(UvCoord, rAspect), 1.0);
	   return;
	}

/* Disable for RetroArch since there's no depth buffer
	// Create parallax effect
	if(ParallaxSwitch)
	{
		float ParallaxDirection = ParallaxOffset*0.01;
		// For stereo-vison flip direction on one side
		ParallaxDirection *= StereoSwitch ? StereoMask : 1.0;
		// Apply parallax effect
		UvCoord = Parallax(
			UvCoord,
			ParallaxDirection,
			ParallaxCenter,
			ParallaxMaskScalar,
			ParallaxSteps
		);
	}
*/

   // added by hunterk to adjust aspect ratio of the image
	vec2 corrected_size = params.SourceSize.xy * vec2(img_ar.x / img_ar.y, 1.0)
		 * vec2(params.SourceSize.y / params.SourceSize.x, 1.0);
	float full_scale = params.OutputSize.y / params.SourceSize.y;
	vec2 scale = (params.OutputSize.xy / corrected_size) / full_scale;
	vec2 middle = vec2(0.49999, 0.49999);
	vec2 diff = UvCoord.xy - middle;
	vec2 screen_coord = middle + diff * scale;
	UvCoord = ((screen_coord - vec2(0.5)) * imgzoom) + vec2(0.5);

	// Sample image with black borders to display
	vec3 Image = mix(
		texture(Source, UvCoord).rgb, // Display image
		vec3(0.0), // Black borders
		BorderMaskAA(UvCoord) // Anti-aliased border mask
	);

	// Display image
   FragColor = vec4(Image, 1.0);
}
