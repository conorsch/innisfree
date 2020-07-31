#!/bin/bash




doctl compute droplet create --region sfo2 --image debian-10-x64 --size s-2vcpu-2gb --user-data-file files/userdata.cfg --wait --output json innisfree

