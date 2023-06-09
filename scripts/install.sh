#!/bin/bash

if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    echo "Install script not available for your OS."
elif [[ "$OSTYPE" == "darwin"* ]]; then
    echo "OSX"
    echo "Fetching the package..."
	curl -s https://api.github.com/repos/raphamorim/rio/releases/latest \
	| grep "macos-rio" \
	| cut -d : -f 2,3 \
	| tr -d \" \
	| wget -qi -
	echo "Moving to /Applications/..."
	unzip -o ./macos-rio -d /Applications/
	echo "Cleaning up..."
	rm ./macos-rio.zip
	echo "Installation done!"
elif [[ "$OSTYPE" == "cygwin" ]]; then
    # POSIX compatibility layer and Linux environment emulation for Windows
    echo "Install script not available for your OS."
elif [[ "$OSTYPE" == "msys" ]]; then
    # Lightweight shell and GNU utilities compiled for Windows (part of MinGW)
    echo "Install script not available for your OS."
elif [[ "$OSTYPE" == "win32" ]]; then
    echo "Install script not available for your OS."
elif [[ "$OSTYPE" == "freebsd"* ]]; then
    echo "Install script not available for your OS."
else
    echo "Unable to define OS type. Finished."
fi
