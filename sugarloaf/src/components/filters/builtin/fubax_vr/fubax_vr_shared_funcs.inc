// Adjust to limited RGB
vec3 tv(vec3 Input)
{ return Input*((235.0-16.0)/255.0)+16.0/255.0; }

// Generate test grid
vec3 Grid(vec2 Coordinates, float AspectRatio)
{
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

   bool RadialPattern = (SoloLines == 3);
   
   vec2 GridCoord = Coordinates;
	GridCoord.y -= 0.5; // Center coordinates vertically
	GridCoord.y /= AspectRatio; // Correct aspect
	GridCoord.y += 0.5; // Center at middle
	
	vec2 CrossUV = GridCoord; // Store center cross coordinates
	
	vec2 PixelSize; vec3 gridColor;
	// Generate grid pattern
	GridCoord = (RadialPattern) ? vec2(length(GridCoord-0.5)*1.618) : GridCoord; // Switch to radial pattern
	GridCoord = abs(fract(GridCoord*BoxAmount)*2.0-1.0)*(thicknessA+1.0);
	// Anti-aliased grid
	PixelSize = fwidth(GridCoord.xy);
	GridCoord = smoothstep(1.0-PixelSize, 1.0+PixelSize, GridCoord);

	// Combine/solo vertical and horizontal lines
	switch(SoloLines)
	{
		case 1:
			{ gridColor = vec3(GridCoord.y); break; }
		case 2:
			{ gridColor = vec3(GridCoord.x); break; }
		default:
			{ gridColor = vec3(max(GridCoord.x, GridCoord.y)); break; }
	};

	// Generate center cross
	CrossUV = 1.0-abs(CrossUV*2.0-1.0);
	CrossUV = CrossUV*(thicknessB/BoxAmount+1.0);
	// Anti-aliased cross
	PixelSize = fwidth(CrossUV);
	CrossUV = smoothstep(1.0-PixelSize, 1.0+PixelSize, CrossUV);
	// Combine vertical and horizontal line
	float CrossMask = max(CrossUV.y, CrossUV.x);

	// Blend grid and center cross
	gridColor = mix(gridColor, ((RadialPattern) ? vec3(1.0) : crossColor), vec3(CrossMask));

	// Solo colors
	if(SoloGreen) gridColor.b = 0.0;
	if(SoloBlue) gridColor.g = 0.0;

	// Reduce grid brightness
	return tv(gridColor);
}

// Divide screen into two halfs
vec2 StereoVision(vec2 Coordinates, float Center)
{
	vec2 StereoCoord = Coordinates;
	StereoCoord.x = 0.25 + abs( StereoCoord.x*2.0-1.0 ) * 0.5; // Divide screen in two
	StereoCoord.x -= mix(-0.25, 0.25, Center); // Change center for interpupillary distance (IPD)
	// Mirror left half
	float ScreenSide = step(0.5, Coordinates.x);
	StereoCoord.x *= ScreenSide*2.0-1.0;
	StereoCoord.x += 1.0 - ScreenSide;
	return StereoCoord;
}

// Convert stereo coordinates to mono
vec2 InvStereoVision(vec2 Coordinates, int ScreenMask, float Center)
{
	vec2 stereoCoord = Coordinates;
	stereoCoord.x += Center*0.5 * ScreenMask;
	return stereoCoord;
}

// Generate border mask with anti-aliasing from UV coordinates
float BorderMaskAA(vec2 Coordinates)
{
	vec2 RaidalCoord = abs(Coordinates*2.0-1.0);
	// Get pixel size in transformed coordinates (for anti-aliasing)
	vec2 PixelSize = fwidth(RaidalCoord);

	// Create borders mask (with anti-aliasing)
	vec2 Borders = smoothstep(1.0-PixelSize, 1.0+PixelSize, RaidalCoord);

	// Combine side and top borders
	return max(Borders.x, Borders.y);
}

/*
// Can't really use this one as RetroArch can't access the depth buffer
float GetDepth(vec2 texcoord)
{
	return ReShade::GetLinearizedDepth(texcoord);
} 


// Horizontal parallax offset effect
vec2 Parallax(vec2 Coordinates, float Offset, float Center, int GapOffset, int Steps)
{
// Limit amount of loop steps to make it finite
#ifndef MaximumParallaxSteps
#def MaximumParallaxSteps 64
#endif

	// Offset per step progress
	float LayerDepth = 1.0 / min(MaximumParallaxSteps, Steps);

	// Netto layer offset change
	float deltaCoordinates = Offset * LayerDepth;

	vec2 ParallaxCoord = Coordinates;
	// Offset image horizontally so that parallax is in the depth appointed center
	ParallaxCoord.x += Offset * Center;
	float CurrentDepthMapValue = GetDepth(ParallaxCoord); // Replace function

	// Steep parallax mapping
	float CurrentLayerDepth = 0.0;
	[loop]
	while(CurrentLayerDepth < CurrentDepthMapValue)
	{
		// Shift coordinates horizontally in linear fasion
		ParallaxCoord.x -= deltaCoordinates;
		// Get depth value at current coordinates
		CurrentDepthMapValue = GetDepth(ParallaxCoord); // Replace function
		// Get depth of next layer
		CurrentLayerDepth += LayerDepth;
		continue;
	}

	// Parallax Occlusion Mapping
	vec2 PrevParallaxCoord = ParallaxCoord;
	PrevParallaxCoord.x += deltaCoordinates;
	float afterDepthValue = CurrentDepthMapValue - CurrentLayerDepth;
	float beforeDepthValue = GetDepth(PrevParallaxCoord); // Replace function
	// Store depth read difference for masking
	float DepthDifference = beforeDepthValue - CurrentDepthMapValue;

	beforeDepthValue += LayerDepth - CurrentLayerDepth;
	// Interpolate coordinates
	float weight = afterDepthValue / (afterDepthValue - beforeDepthValue);
	ParallaxCoord = PrevParallaxCoord * weight + ParallaxCoord * (1.0 - weight);

	// Apply gap masking (by JMF)
	DepthDifference *= GapOffset * Offset * 100.0;
	DepthDifference *= ReShade::PixelSize.x; // Replace function
	ParallaxCoord.x += DepthDifference;

	return ParallaxCoord;
};
*/

// Lens projection model (algorithm by JMF)
float Orthographic(float rFOV, float R){ return tan(asin(sin(rFOV*0.5)*R))/(tan(rFOV*0.5)*R); }
float Equisolid(float rFOV, float R){ return tan(asin(sin(rFOV*0.25)*R)*2.0)/(tan(rFOV*0.5)*R); }
float Equidistant(float rFOV, float R){ return tan(R*rFOV*0.5)/(tan(rFOV*0.5)*R); }
float Stereographic(float rFOV, float R){ return tan(atan(tan(rFOV*0.25)*R)*2.0)/(tan(rFOV*0.5)*R); }

// Brown-Conrady radial distortion model (multiply by coordinates)
float kRadial(float R2, float K1, float K2, float K3, float K4)
{ return 1.0 + K1*R2 + K2*pow(R2,2) + K3*pow(R2,4) + K4*pow(R2,6); }

// Brown-Conrady tangental distortion model (add to coordinates)
vec2 pTangental(vec2 Coord, float R2, float P1, float P2, float P3, float P4)
{
	return vec2(
		(P1*(R2+pow(Coord.x,2)*2.0)+2.0*P2*Coord.x*Coord.y)*(1.0+P3*R2+P4*pow(R2,2)),
		(P2*(R2+pow(Coord.y,2)*2.0)+2.0*P1*Coord.x*Coord.y)*(1.0+P3*R2+P4*pow(R2,2))
	);
}

// RGB to YUV709.luma
float Luma(vec3 Input)
{
	const vec3 Luma709 = vec3(0.2126, 0.7152, 0.0722);
	return dot(Input, Luma709);
}

// Overlay blending mode
float Overlay(float LayerA, float LayerB)
{
	float MinA = min(LayerA, 0.5);
	float MinB = min(LayerB, 0.5);
	float MaxA = max(LayerA, 0.5);
	float MaxB = max(LayerB, 0.5);
	return 2.0 * (MinA * MinB + MaxA + MaxB - MaxA * MaxB) - 1.5;
}
