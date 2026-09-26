// Shows the demo's art file, redrawing whenever the file changes.
//
// It follows demo-art.jpg at the top of the repository, the art_file the demo
// writes. To follow another file, set DEMO_ART when starting Processing:
//
//   DEMO_ART=/path/to/art.jpg \
//     /Applications/Processing.app/Contents/MacOS/Processing cli \
//     --sketch=tools/DemoArtViewer --run

String ART;

PImage art;
long seen = -1;

void setup() {
  size(512, 512);
  ART = artPath();
  surface.setTitle("panel-ddp art");
  noSmooth();  // the scaled-up panel version stays crisp; originals are big enough not to care
  background(0);
}

void draw() {
  File f = new File(ART);
  long modified = f.exists() ? f.lastModified() : -1;
  if (modified != seen) {
    PImage next = f.exists() ? loadImage(ART) : null;  // null while half-written; try again next frame
    if (next != null || !f.exists()) {
      art = next;
      seen = modified;
    }
  }
  background(0);
  if (art != null) {
    image(art, 0, 0, width, height);
  }
}

// $DEMO_ART, else demo-art.jpg two folders up from the sketch.
String artPath() {
  String env = System.getenv("DEMO_ART");
  if (env != null && !env.isEmpty()) return env;
  return sketchPath("../../demo-art.jpg");
}
