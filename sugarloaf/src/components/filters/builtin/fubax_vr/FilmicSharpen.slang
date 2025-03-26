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
   vec2 UvCoord = texcoord;
	// Bypass sharpening
	if(!Sharpen)
	{
	   FragColor = vec4(texture(Source, UvCoord).rgb, 1.0);
	   return;
	}
	
	vec2 Pixel = (texcoord.xy * params.OutputSize.xy) * Offset;
	// Sample display image
	vec3 ImgSource = texture(Source, UvCoord).rgb;

	vec2 NorSouWesEst[4] = {
		vec2(UvCoord.x, UvCoord.y + Pixel.y),
		vec2(UvCoord.x, UvCoord.y - Pixel.y),
		vec2(UvCoord.x + Pixel.x, UvCoord.y),
		vec2(UvCoord.x - Pixel.x, UvCoord.y)
	};

	// Luma high-pass
	float HighPass = 0.0;
	for(int i=0; i<4; i++)
	{
	   HighPass += Luma(texture(Source, NorSouWesEst[i]).rgb);
	}
	HighPass = 0.5 - 0.5 * (HighPass * 0.25 - Luma(ImgSource));

	// Sharpen strength
	HighPass = mix(0.5, HighPass, Strength * 0.01);

	// Clamping sharpen
	HighPass = (Clamp != 1.0) ? max(min(HighPass, Clamp), 1.0 - Clamp) : HighPass;

	vec3 Sharpen = vec3(
		Overlay(ImgSource.r, HighPass),
		Overlay(ImgSource.g, HighPass),
		Overlay(ImgSource.b, HighPass)
	);

   FragColor.rgb = (Preview) ? vec3(HighPass) : Sharpen;
   FragColor.a = 1.0;
}
