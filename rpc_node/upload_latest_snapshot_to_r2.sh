#!/bin/bash

# Name of Cloudflare remote configuration
cloudflare_r2_remote_config="cloudflare-r2"
# Cloudflare R2 bucket name
bucket_name="moonshot-snapshots"

# Path to the config.toml file in the RPC node home folder
# TODO: update based on actual config file location for rpc node
config_file="./config.toml"

# Extract the snapshot_path from the config.toml file
snapshot_dir=$(grep -oP '(?<=snapshot_path = ").*(?=")' "$config_file")

# Check if the snapshot directory exists
if [ ! -d "$snapshot_dir" ]; then
  echo "Snapshot directory not found: $snapshot_dir"
  exit 1
fi

# Path to the latest_snapshot_timestamp file
latest_snapshot_timestamp_file="$snapshot_dir/latest_snapshot_timestamp"

# Check if the latest_snapshot_timestamp file exists
if [ ! -f "$latest_snapshot_timestamp_file" ]; then
  echo "Latest snapshot timestamp file not found: $latest_snapshot_timestamp_file"
  exit 1
fi

# Read the latest snapshot timestamp from the file
timestamp=$(cat "$latest_snapshot_timestamp_file")

# Construct the latest snapshot folder path
latest_snapshot_folder="$snapshot_dir/snapshot_$timestamp"

# Check if the latest snapshot folder exists
if [ ! -d "$latest_snapshot_folder" ]; then
  echo "Latest snapshot folder not found: $latest_snapshot_folder"
  exit 1
fi

# Upload the latest snapshot folder to Cloudflare R2
rclone sync "$latest_snapshot_folder" "$cloudflare_r2_remote_config:$bucket_name/snapshot_$timestamp" --progress