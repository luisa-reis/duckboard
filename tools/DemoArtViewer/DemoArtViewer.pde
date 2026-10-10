// Shows the demo's art file, redrawing whenever the file changes, and black
// while there is no file.
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
  size(640, 640);
  ART = artPath();
  surface.setTitle("duckboard art");
  noSmooth();  // the scaled-up panel version stays crisp; originals are big enough not to care
  background(0);
}

void draw() {
  File f = new File(ART);
  if (!f.exists()) {
    art = null;
    seen = -1;
  } else if (f.lastModified() != seen) {
    PImage next = loadImage(ART);  // null while half-written; try again next frame
    if (next != null) {
      art = next;
      seen = f.lastModified();
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
