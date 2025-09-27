#!/bin/bash

DISPLAY_OUTPUT="DP-1"
SHIFT_X=1
SHIFT_Y=1
LOG_FILE="./display_transform.log"  # You can change this path to wherever you want your logs

# Log function to add timestamp and log messages
log_message() {
    echo "$(date "+%Y-%m-%d %H:%M:%S") - $1" >> $LOG_FILE
}

log_message "Script started."

while true; do
    # Log the start of the transformation
    log_message "Applying transformation: SHIFT_X=$SHIFT_X, SHIFT_Y=$SHIFT_Y"

    # Apply the transformation
    xrandr --output "$DISPLAY_OUTPUT" --transform 1,0,$SHIFT_X,0,1,$SHIFT_Y,0,0,1
    sleep 20

    # Log the reset state
    log_message "Resetting transformation to default."

    # Reset the transformation
    xrandr --output "$DISPLAY_OUTPUT" --transform 1,0,0,0,1,0,0,0,1
    sleep 20
done

