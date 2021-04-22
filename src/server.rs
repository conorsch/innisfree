// extern crate digitalocean;
// use digitalocean::prelude::*;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::thread;
use std::time;

// For file reading
use std::fs::File;
use std::io;
use std::io::prelude::*;

// Web API request imports, see
// https://rust-lang-nursery.github.io/rust-cookbook/web/clients/apis.html
extern crate reqwest;

extern crate serde;
extern crate serde_json;
use serde::Deserialize;
use serde_json::json;

const DO_REGION: &str = "sfo2";
const DO_SIZE: &str = "s-1vcpu-1gb";
const DO_IMAGE: &str = "ubuntu-20-04-x64";
// const DO_NAME: &str = "innisfree";
const DO_NAME: &str = "jawn";
const DO_API_BASE_URL: &str = "https://api.digitalocean.com/v2/droplets";

// Representation of a DigitalOcean Droplet, i.e. cloud VM.
#[derive(Debug, Deserialize)]
pub struct Droplet {
    id: u32,
    name: String,
    // Field are attributes, will need to retrieve them as foo.slug
    // region: String,
    // size: String,
    // image: String,
    status: String,
    user_data: Option<String>,
    raw_json: Option<String>,
    networks: HashMap<String, Vec<HashMap<String, String>>>,
}

// The 'networks' field in the API response will be a nested object,
// so let's stub that out so a Droplet can have a .networks field.
#[derive(Debug, Deserialize)]
struct Network {
    ip_address: String,
    netmask: String,
    /// *Note:* Since `type` is a keyword in Rust `kind` is used instead.
    #[serde(rename = "type")]
    kind: String,
    // Gateway won't exist on private, could be an Option<String>
    // gateway: String,
}

#[derive(Debug, Deserialize)]
struct DropletResponse {
    droplet: Droplet,
}

impl Droplet {
    // IPv4 lookup can fail, should return Result to force handling.
    pub fn ipv4_address(&self) -> String {
        let mut ip: String = "".to_string();
        for v4_network in &self.networks["v4"] {
            if v4_network["type"] == "public" {
                ip = v4_network["ip_address"].clone();
                break;
            }
        }
        return ip.to_string();
    }
}

fn get_droplet(droplet: &Droplet) -> Droplet {
    let api_key = env::var("DIGITALOCEAN_API_TOKEN").expect("DIGITALOCEAN_API_TOKEN not set.");
    let request_url = DO_API_BASE_URL.to_owned() + "/" + &droplet.id.to_string();

    let client = reqwest::blocking::Client::new();
    let response = client.get(request_url).bearer_auth(api_key).send().unwrap();

    let j: serde_json::Value = response.json().unwrap();
    let d: String = j["droplet"].to_string();
    let droplet_new: Droplet = serde_json::from_str(&d).unwrap();
    return droplet_new;
}

pub fn get_user_data() -> String {
    let user_data = include_str!("../files/cloudinit.cfg");
    let user_data = user_data.to_string();
    return user_data;
}

fn get_mock_droplet_json() -> String {
    let mut droplet_json = include_str!("../files/droplet.json");
    let droplet_json = droplet_json.to_string();
    return droplet_json;
}

pub fn create_droplet() -> Droplet {
    let droplet_json = get_mock_droplet_json();
    let droplet: Droplet = serde_json::from_str(&droplet_json).unwrap();
    return droplet;
}

#[allow(dead_code)]
pub fn _create_droplet() -> Droplet {
    // Build JSON request body, for sending to DigitalOcean API
    let droplet_body = json!({
        "image": DO_IMAGE,
        "name": DO_NAME,
        "region": DO_REGION,
        "size": DO_SIZE,
    });

    let api_key = env::var("DIGITALOCEAN_API_TOKEN").expect("DIGITALOCEAN_API_TOKEN not set.");
    let request_url = DO_API_BASE_URL;
    let client = reqwest::blocking::Client::new();

    let response = client
        .post(request_url)
        .json(&droplet_body)
        .bearer_auth(api_key)
        .send()
        .unwrap();
    println!("RESP: {:?}", response);

    let j: serde_json::Value = response.json().unwrap();
    let d: String = j["droplet"].to_string();
    println!("DROPLET JAWN {:?}", d);
    let droplet: Droplet = serde_json::from_str(&d).unwrap();
    loop {
        let droplet: Droplet = get_droplet(&droplet);
        if droplet.status == "active" {
            return droplet;
        } else {
            info!("Droplet still booting, waiting...");
            thread::sleep(time::Duration::from_secs(10));
        }
    }
}
