/*
 * Umbrella header fed to bindgen.
 *
 * `gphoto2.h` transitively pulls in the port, file, library, setting and
 * (via gphoto2-library.h -> gphoto2-camera.h) the camera/abilities/widget
 * headers. We additionally include the headers whose symbols we rely on
 * directly so the binding surface is explicit and robust against upstream
 * reshuffling its transitive includes.
 */
#include <gphoto2/gphoto2.h>
#include <gphoto2/gphoto2-camera.h>
#include <gphoto2/gphoto2-abilities-list.h>
#include <gphoto2/gphoto2-port-info-list.h>
#include <gphoto2/gphoto2-version.h>
#include <gphoto2/gphoto2-result.h>
