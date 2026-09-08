# Sourced by lab scripts after setting lab_dir. Keep caller cwd and shell options.
lab_cargo_manifest() {
  lab_manifest=$1
  lab_command=$2
  shift 2
  env -u CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER cargo "$lab_command" \
    --manifest-path "$lab_manifest" --locked --offline -j 2 "$@"
}

lab_cargo() {
  lab_cargo_manifest "$lab_dir/Cargo.toml" "$@"
}

lab_temp() {
  mktemp -d "${TMPDIR:-/tmp}/falcon-$1.XXXXXX"
}

lab_encode() {
  lab_input=$1
  lab_output=$2
  shift 2
  ffmpeg -hide_banner -loglevel error -y -i "$lab_input" -an "$@" \
    -c:v libx264 -threads 2 -preset veryfast -crf 20 \
    -pix_fmt yuv420p -movflags +faststart "$lab_output"
}

lab_probe() {
  ffprobe -v error -select_streams v:0 \
    -show_entries stream=codec_name,width,height,nb_frames,duration:format=duration,size \
    -of json "$1"
}
