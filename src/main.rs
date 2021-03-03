use cfg_if::cfg_if;
use geo::Coord;
use leaflet::Map;
use osm::{OsmDocument, OsmNode, OsmWay};
use seed::{prelude::*, *};

mod geo;
mod map;
mod osm;

pub struct Model {
    map: Option<Map>,
    osm: OsmDocument,
    position: Option<Coord>,
}

enum Msg {
    SetMap(Map),
    OsmFetched(fetch::Result<String>),
}

fn init(_: Url, orders: &mut impl Orders<Msg>) -> Model {
    // Cannot initialize Leaflet until the map element has rendered.
    orders.after_next_render(|_| {
        let map_view = map::init();
        Msg::SetMap(map_view)
    });

    orders
        .skip()
        .perform_cmd(async { Msg::OsmFetched(send_osm_request().await) });

    Model {
        map: None,
        osm: OsmDocument::new(),
        position: Some(Coord {
            lat: 63.4015,
            lon: 10.2935,
        }),
    }
}

fn get_osm_request_url() -> &'static str {
    "https://www.openstreetmap.org/api/0.6/map?bbox=10.29072%2C63.39981%2C10.29426%2C63.40265"
}

async fn send_osm_request() -> fetch::Result<String> {
    fetch(get_osm_request_url())
        .await?
        .check_status()?
        .text()
        .await
}

fn update(msg: Msg, model: &mut Model, _: &mut impl Orders<Msg>) {
    match msg {
        Msg::SetMap(map) => {
            model.map = Some(map);
            map::set_view(&model);
            map::render_topology(&model);
            map::render_position(&model);
        }

        Msg::OsmFetched(Ok(response_data)) => {
            model.osm = quick_xml::de::from_str(&response_data)
                .expect("Unable to deserialize the OSM data");

            map::render_topology(&model);
        }

        Msg::OsmFetched(Err(fetch_error)) => {
            error!("Fetching OSM data failed: {:#?}", fetch_error);
        }
    }
}

fn view(model: &Model) -> Node<Msg> {
    div![
        div![id!["map"]],
        div![model.osm.ways.iter().map(|way| view_way(&model, way))],
    ]
}

fn view_way(model: &Model, way: &OsmWay) -> Node<Msg> {
    div![
        h2![&way.id],
        match &model.position {
            Some(pos) => {
                div![format!("Distance = {}", way.distance(&pos, &model.osm))]
            }
            None => {
                div![]
            }
        },
        ul![way
            .tags
            .iter()
            .map(|tag| li![format!("{} = {}", tag.k, tag.v)])],
    ]
}

cfg_if! {
    if #[cfg(debug_assertions)] {
        fn init_log() {
            use log::Level;
            console_log::init_with_level(Level::Trace).expect("error initializing log");
        }
    } else {
        fn init_log() {}
    }
}

fn main() {
    init_log();
    App::start("app", init, update, view);
}

impl Model {
    fn nearest_points(&self) -> Vec<(f64, Coord, &OsmWay)> {
        let mut nearest_points = vec![];

        if let Some(pos) = &self.position {
            for way in self.osm.ways.iter() {
                let nodes: Vec<&OsmNode> = way.points(&self.osm).collect();
                let mut node_distances = vec![];

                for (i, &line1) in nodes.iter().enumerate() {
                    if i < nodes.iter().count() - 1 {
                        let line2 = nodes[i + 1];

                        let length = geo::distance(&line1.into(), &line2.into());
                        let along_track_distance =
                            geo::along_track_distance(&line1.into(), &line2.into(), &pos);

                        let bearing = geo::bearing(&line1.into(), &line2.into());

                        let destination = if along_track_distance < 0.0 {
                            line1.into()
                        } else if along_track_distance > length {
                            line2.into()
                        } else {
                            geo::destination(&line1.into(), bearing, along_track_distance)
                        };

                        let distance = geo::distance(pos, &destination);

                        node_distances.push((distance, destination, way));
                    }
                }

                let nearest_point = node_distances
                    .into_iter()
                    .min_by(|(x, _, _), (y, _, _)| {
                        x.partial_cmp(y).expect("Could not compare distances")
                    })
                    .expect("Could not find a nearest distance");

                nearest_points.push(nearest_point);
            }
        }

        nearest_points
    }

    fn nearest_way(&self) -> &OsmWay {
        let nearest_points = self.nearest_points();

        let nearest_point = nearest_points
            .iter()
            .min_by(|(x, _, _), (y, _, _)| x.partial_cmp(y).expect("Could not compare distances"))
            .expect("Could not find a nearest distance");

        let (_, _, way) = nearest_point;
        way
    }
}
