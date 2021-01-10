#!/bin/bash

ARTEFACT_DIRECTORY=artefact
LIB_PATH_PREFIX=target/release/
#LIB_PATH_PREFIX=target/debug/

for BUNDLE_LIB in "NoteGenerator note_generator" "NoteOffDelay note_off_delay" "FilterOutNonNote filter_out_non_note" \
 " NoteFanOut note_fan_out" "MidiDelay midi_delay" "MaxNoteDuration max_note_duration" "AudioData audio_data" \
 " ArpegiatorPatternReceiver arpegiator_pattern_receiver" "Arpegiator arpegiator" ; do
  BUNDLE_NAME=$(echo $BUNDLE_LIB | cut -f1 -d" ")
  LIBNAME=$(echo $BUNDLE_LIB | cut -f2 -d" ")
  LIB_PATH=${LIB_PATH_PREFIX}lib${LIBNAME}.dylib

  # Make the bundle folder
  mkdir -p "${ARTEFACT_DIRECTORY}/${BUNDLE_NAME}.vst/Contents/MacOS"

  # Create the PkgInfo
  echo "BNDL????" > "${ARTEFACT_DIRECTORY}/${BUNDLE_NAME}.vst/Contents/PkgInfo"

  #build the Info.Plist
  echo "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
  <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">
  <plist version=\"1.0\">
  <dict>
      <key>CFBundleDevelopmentRegion</key>
      <string>English</string>

      <key>CFBundleExecutable</key>
      <string>${BUNDLE_NAME}</string>

      <key>CFBundleGetInfoString</key>
      <string>vst</string>

      <key>CFBundleIconFile</key>
      <string></string>

      <key>CFBundleIdentifier</key>
      <string>com.rust-vst.${BUNDLE_NAME}</string>

      <key>CFBundleInfoDictionaryVersion</key>
      <string>6.0</string>

      <key>CFBundleName</key>
      <string>${BUNDLE_NAME}</string>

      <key>CFBundlePackageType</key>
      <string>BNDL</string>

      <key>CFBundleVersion</key>
      <string>1.$((RANDOM % 9999))</string>

      <key>CFBundleSignature</key>
      <string>$((RANDOM % 9999))</string>

      <key>CSResourcesFileMapped</key>
      <string></string>

  </dict>
  </plist>" > "${ARTEFACT_DIRECTORY}/${BUNDLE_NAME}.vst/Contents/Info.plist"

  # move the provided library to the correct location
  cp "${LIB_PATH}" "${ARTEFACT_DIRECTORY}/${BUNDLE_NAME}.vst/Contents/MacOS/${BUNDLE_NAME}"

  echo "Created bundle ${BUNDLE_NAME}.vst"
done
