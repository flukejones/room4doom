use self::{defs::DrawSeg, planes::VisPlaneRender, portals::PortalClip};
use gameplay::Angle;

pub struct OpenglRenderer {}

impl PlayRenderer for SoftwareRenderer {
    fn render_player_view(&mut self, player: &Player, level: &Level, pixels: &mut PixelBuf) {}
}
