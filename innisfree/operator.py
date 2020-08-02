import kopf
import kubernetes.client
from pathlib import Path
from kubernetes.client.rest import ApiException
from ruamel import yaml
import os

from .utils import logger


@kopf.on.create("ruin.dev", "v1", "innisfree")
def create_fn(spec, name, **kwargs):
    logger.debug(f"Creating Innisfree tunnel {name}")
    project_root = Path(__file__).parent.parent
    create_deployment()
    create_service()


def create_deployment():

    doc_templ = os.path.join(project_root, "files", "k8s", "deployment.yml")

    with open(doc_templ, "r") as f:
        doc = yaml.round_trip_load(f, preserve_quotes=True)

    kopf.adopt(doc)

    api = kubernetes.client.AppsV1Api()
    try:
        resource = api.create_namespaced_deployment(namespace=doc["metadata"]["namespace"], body=doc)
        return {"children": [doc.metadata.uid]}
    except ApiException as e:
        logger.error(f"Failed to create tunnel deployment: {e}")
        raise


def create_service():
    doc_templ = os.path.join(project_root, "files", "k8s", "service.yml")

    with open(doc_templ, "r") as f:
        doc = yaml.round_trip_load(f, preserve_quotes=True)

    doc["status"] = {}
    doc["status"]["loadBalancer"] = {}
    doc["status"]["loadBalancer"]["ingress"] = [{"ip": "4.4.4.4"}]

    kopf.adopt(doc)

    api = kubernetes.client.AppsV1Api()
    try:
        resource = api.create_namespaced_service(namespace=doc["metadata"]["namespace"], body=doc)
        return {"children": [doc.metadata.uid]}
    except ApiException as e:
        logger.error(f"Failed to create tunnel service: {e}")
        raise
