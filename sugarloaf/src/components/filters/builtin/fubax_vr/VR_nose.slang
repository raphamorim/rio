#version 450

/*
Nose PS (c) 2019 Jacob Maximilian Fober

Anti-nausea shader for VR

This work is licensed under the Creative Commons 
Attribution-ShareAlike 4.0 International License. 
To view a copy of this license, visit 
http://creativecommons.org/licenses/by-sa/4.0/.
*/

layout(push_constant) uniform Push
{
	vec4 SourceSize;
	vec4 OriginalSize;
	vec4 OutputSize;
	uint FrameCount;
} params;

#ifndef nose
#define nose 128 // Nose texture resolution
#endif

#define mul(c,d) (d*c)

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
layout(set = 0, binding = 3) uniform sampler2D noseTex;

//#include "fubax_vr_shared_funcs.inc"
// Convert RGB to YUV
vec3 yuv(vec3 rgbImage)
{
	// RGB to YUV709 matrix
	const mat3 YUV709 =
	mat3(
		vec3(0.2126, 0.7152, 0.0722),
		vec3(-0.09991, -0.33609, 0.436),
		vec3(0.615, -0.55861, -0.05639)
	);
	return mul(YUV709, rgbImage);
}

// Overlay blending mode
float Overlay(float LayerAB)
{
	float MinAB = min(LayerAB, 0.5);
	float MaxAB = max(LayerAB, 0.5);
	return 2.0 * (MinAB*MinAB + 2.0*MaxAB - MaxAB*MaxAB) - 1.5;
}

void main()
{
   vec2 UvCoord = texcoord;
	// Bypass sharpening
	if(!Nose)
	{
	   FragColor = vec4(texture(Source, UvCoord).rgb, 1.0);
	   return;
	}
	
	// Divide screen in two (mirrored)
	vec2 StereoCoord = texcoord;
	StereoCoord.x = 1.0-abs(StereoCoord.x*2.0-1.0)/Scale.x;
	StereoCoord.y = 1.0-(1.0-StereoCoord.y)/Scale.y;

	// Sample display image
	vec3 Display = texture(Source, texcoord).rgb;
	// Sample nose texture
	vec4 NoseTexture = texture(noseTex, StereoCoord);
	// Change skintone
	NoseTexture.rgb *= mix(smoothstep(0.0, 1.0, yuv(NoseTexture.rgb).x), 1.0, Brightness);


	// Blend nose with display image
	FragColor.rgb = mix(Display, NoseTexture.rgb, NoseTexture.a);
   FragColor.a = 1.0;
}
