#!/bin/bash
#
# to set environment variables for c++ stdlib paths:
# $ source build/envsetup.sh 
#
# to automatically select the highest version of the c++ stdlib:
# $ source build/envsetup.sh highest

# console colors
RED='\033[0;31m'
GRN='\033[0;32m'
ORNG='\033[0;33m'
NC='\033[0;0m'
USE_HIGHEST=0
if [ "$1" == "highest" ]; then
    USE_HIGHEST=1
fi

# find the highest number in an array
select_highest_number() {
    local numbers=("$@")
    local highest_number=0

    for number in "${numbers[@]}"; do
        if [ "$number" -gt "$highest_number" ]; then
            highest_number=$number
        fi
    done

    echo "$highest_number"
}

# Select a directory from a list of directories
select_directory() {
    local directories=("$@")
    local selected_path=""
    # Check if the directory array contains numbers or strings
    is_number=0
    if [[ "${directories[0]}" =~ ^[0-9]+$ ]]; then
        is_number=1
    fi

    # Check the number of directories
    if [ "${#directories[@]}" -eq 1 ]; then
        # If only one directory is found, set MY_PATH to it
        selected_path="${directories[0]}"

    elif [ "$USE_HIGHEST" -eq 1 ] && [ "$is_number" -eq 1 ]; then
        highest_version=$(select_highest_number "${directories[@]}")
        selected_path="$highest_version"

    else
        # Display directories to the user if more than one exists
        printf "Select a directory: \n" 1>&2
        for i in "${!directories[@]}"; do
            printf "%d) %s\n" "$((i + 1))" "${directories[i]}" 1>&2
        done

        # Prompt the user to enter a choice
        read -p "Enter the number of your choice: " choice

        # Validate the user’s input
        if [[ "$choice" -ge 1 && "$choice" -le "${#directories[@]}" ]]; then
            selected_path="${directories[choice - 1]}"
        else
            printf "${RED}Invalid selection ${NC}\n" 1>&2
            return 1
        fi
    fi

    # Return the selected path
    echo "$selected_path"
}

# find c++ include
if [ -d "/usr/include/c++/" ]; then
    CPP_DIRS=$( ls /usr/include/c++/ )
    CPP_ARRAY=($CPP_DIRS)
    STDLIB_DIR=$(select_directory "${CPP_ARRAY[@]}")
    if [ -z "$STDLIB_DIR" ]; then 
        return 1
    fi
    STDLIB_PATH="/usr/include/c++/${STDLIB_DIR}"
else
    # Print warning if the directory doesn't exist
    echo -e "${RED}/usr/include/c++/ does not exist.${NC}"
    return
fi

# find machine dir
MACHINE_NAME=$(uname -m)
MACHINE_DIRS=$( ls /usr/include/ | grep "${MACHINE_NAME}" )
MACHINE_ARRAY=($MACHINE_DIRS)
ARCH_DIR=$(select_directory "${MACHINE_ARRAY[@]}")
if [ -z "$ARCH_DIR" ]; then 
	return 1
fi
ARCH_PATH="/usr/include/${ARCH_DIR}"

# find arch c++ include
if [ -d "$ARCH_PATH/c++/" ]; then
    CPP_DIRS=$( ls ${ARCH_PATH}/c++/ )
    CPP_ARRAY=($CPP_DIRS)    
    STDLIB_DIR=$(select_directory "${CPP_ARRAY[@]}")
    if [ -z "$STDLIB_DIR" ]; then 
        return 1
    fi
    ARCH_STDLIB_PATH="${ARCH_PATH}/c++/${STDLIB_DIR}"
else
    # Print warning if the directory doesn't exist
    echo -e "${RED}$ARCH_PATH/c++/ does not exist.${NC}"
    return
fi

# export the variables
export GENERIC_CPP_STDLIB_PATH=$STDLIB_PATH
export ARCH_SPECIFIC_CPP_STDLIB_PATH=$ARCH_STDLIB_PATH

echo -e "${ORNG}Set GENERIC_CPP_STDLIB_PATH=${GRN}$GENERIC_CPP_STDLIB_PATH${NC}"
echo -e "${ORNG}Set ARCH_SPECIFIC_CPP_STDLIB_PATH=${GRN}$ARCH_SPECIFIC_CPP_STDLIB_PATH${NC}"
