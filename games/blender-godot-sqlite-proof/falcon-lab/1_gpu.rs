// Game-owned pixel assertions around the shared recording implementation.
pub use game_capture::{Vertex, WIDTH, line};
pub struct Capture(game_capture::Capture);
impl Capture {
    pub fn new(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self(game_capture::Capture::new(path)?))
    }
    pub fn frame(&mut self, vertices: &[Vertex]) -> Result<(), Box<dyn std::error::Error>> {
        self.frame_regions(vertices, [[50, 600, 130, 445], [580, 690, 150, 270]])
    }
    pub fn frame_regions(
        &mut self,
        vertices: &[Vertex],
        regions: [[u32; 4]; 2],
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.0.frame_checked(vertices, |data| {
            // Colored geometry must survive actual GPU readback on every frame.
            let pixels = data.as_chunks::<4>().0;
            assert!(
                pixels
                    .iter()
                    .enumerate()
                    .filter(|(i, p)| {
                        let x = *i as u32 % WIDTH;
                        let y = *i as u32 / WIDTH;
                        (regions[0][0]..regions[0][1]).contains(&x)
                            && (regions[0][2]..regions[0][3]).contains(&y)
                            && p[0] > 120
                            && p[0] < 220
                            && p[1] < 160
                            && p[2] > 190
                    })
                    .count()
                    > 100,
                "Falcon geometry absent"
            );
            assert!(
                pixels
                    .iter()
                    .enumerate()
                    .filter(|(i, p)| {
                        let x = *i as u32 % WIDTH;
                        let y = *i as u32 / WIDTH;
                        (regions[1][0]..regions[1][1]).contains(&x)
                            && (regions[1][2]..regions[1][3]).contains(&y)
                            && ((p[0] < 100 && p[1] > 180 && p[2] > 180)
                                || (p[0] > 220 && p[1] > 70 && p[1] < 140 && p[2] < 90))
                    })
                    .count()
                    > 30,
                "sandbag geometry absent"
            );
        })
    }
    pub fn finish(self) -> Result<(), Box<dyn std::error::Error>> {
        self.0.finish()
    }
}
