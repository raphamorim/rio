#!/bin/bash

# Test script for kitty graphics protocol in Rio

# Function to send graphics command
send_graphics() {
    printf "\033_G%s\033\\" "$1"
}

# Test 1: Query support
echo "Testing kitty graphics protocol support..."
send_graphics "a=q,i=1,s=1,v=1,f=24;AAAA"
printf "\033[c"  # Send device attributes query
sleep 0.5

# Test 2: Send a small red square (10x10 pixels, RGB format)
echo -e "\nSending a red square..."
# Create 10x10 red pixels (300 bytes of RGB data)
red_pixels=""
for i in {1..100}; do
    red_pixels="${red_pixels}\377\000\000"  # RGB: 255,0,0
done
# Base64 encode the data
red_base64=$(printf "%b" "$red_pixels" | base64 | tr -d '\n')
send_graphics "a=T,f=24,s=10,v=10,i=100;$red_base64"

# Test 3: Send a blue square using PNG format
echo -e "\nSending a blue square (PNG)..."
# Create a simple 10x10 blue PNG using ImageMagick if available
if command -v convert &> /dev/null; then
    convert -size 10x10 xc:blue /tmp/blue.png
    blue_base64=$(base64 < /tmp/blue.png | tr -d '\n')
    send_graphics "a=T,f=100,i=101;$blue_base64"
    rm -f /tmp/blue.png
else
    echo "ImageMagick not found, skipping PNG test"
fi

# Test 4: Place an existing image
echo -e "\nPlacing image with id=100..."
send_graphics "a=p,i=100,c=2,r=2"

# Test 5: Delete all images
echo -e "\nDeleting all images..."
send_graphics "a=d"

echo -e "\nTest complete!"