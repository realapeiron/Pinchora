# Pinchora on Kubernetes

Kustomize layout:

```
base/            namespace, config, gateway StatefulSet, interactions Deployment, http-proxy
overlays/dev/    one gateway cluster + in-cluster Postgres/Redis, image pinchora:dev
overlays/prod/   4 gateway clusters over 16 shards, ghcr.io image, external databases
```

Apply by hand (not in the kustomization): `base/register-commands-job.yaml`,
`base/ingress.example.yaml`.

## Try it on kind or minikube

```sh
# 1. Build the image and load it into the cluster.
docker build -t pinchora:dev .
kind load docker-image pinchora:dev          # minikube: minikube image load pinchora:dev

# 2. Secrets (gitignored). Dev only needs the Discord values; the overlay points
#    the database and Redis URLs at its own in-cluster instances.
cp deploy/k8s/base/secrets.env.example deploy/k8s/base/secrets.env
$EDITOR deploy/k8s/base/secrets.env

# 3. Deploy.
kubectl apply -k deploy/k8s/overlays/dev
kubectl -n pinchora get pods -w
```

The first pods may restart once or twice while Postgres starts: their `migrate`
init container fails until the database accepts connections.

## Register the slash commands

Run it once after the first deploy and whenever the commands change. The Job reuses
the image of the running gateway:

```sh
IMAGE=$(kubectl -n pinchora get sts pinchora-gateway \
          -o jsonpath='{.spec.template.spec.containers[0].image}')
kubectl -n pinchora delete job register-commands --ignore-not-found
sed "s#image: pinchora\$#image: ${IMAGE}#" deploy/k8s/base/register-commands-job.yaml \
  | kubectl apply -f -
kubectl -n pinchora logs -f job/register-commands
```

The Job passes `--global`. Remove it from the manifest to register to
`PINCHORA_DISCORD__DEV_GUILD_ID` instead (updates instantly, good for development).

## Production

1. Run Postgres (CloudNativePG recommended) and a managed Redis/Valkey, and put their
   URLs in `base/secrets.env`.
2. Set the release in `overlays/prod/kustomization.yaml` (`images[0].newTag`, currently
   the deliberately invalid `<tag>`), or with standalone kustomize:
   `cd overlays/prod && kustomize edit set image pinchora=ghcr.io/realapeiron/pinchora:v0.1.0`.
3. `kubectl apply -k deploy/k8s/overlays/prod`.
4. Interactions endpoint (optional, instead of receiving interactions over the
   gateway): copy `base/ingress.example.yaml`, set your host and TLS secret, apply it,
   then enter `https://<host>/interactions` as the Interactions Endpoint URL in the
   Developer Portal.

After changing `secrets.env` or the ConfigMap, roll the pods (the generated names have no
hash suffix, so nothing restarts by itself):

```sh
kubectl -n pinchora rollout restart deploy/pinchora-interactions deploy/pinchora-http-proxy sts/pinchora-gateway
```

## Scaling rules

**Gateway: replicas == `PINCHORA_GATEWAY__CLUSTER_COUNT`.** Pod `pinchora-gateway-N` is
cluster N and serves the shards of bucket N out of CLUSTER_COUNT. Both numbers sit next
to each other in `base/gateway.yaml` and are patched together in each overlay. If they
differ, shards are skipped or served twice. Never scale the StatefulSet with
`kubectl scale` alone.

**`PINCHORA_GATEWAY__SHARDS` is the total across all clusters, and it is fixed.**
- It is required as soon as CLUSTER_COUNT > 1, so every pod agrees on the total.
  With a single cluster you may leave it unset (Discord's recommendation is used).
- Pick a multiple of the bot's `max_concurrency` (`session_start_limit` in
  `GET /gateway/bot`; 1 for small bots, 16 for very large ones) and leave headroom
  for growth. Changing it reshards the whole bot: every shard re-identifies.
- To add capacity, raise replicas and CLUSTER_COUNT together while SHARDS stays put
  (4 clusters x 4 shards -> 8 clusters x 2 shards). Each cluster then serves
  fewer shards.

**Interactions** is stateless: the HPA scales it between 2 and 20 on CPU (70%). Its
`replicas` field is the floor, so keep it equal to the HPA `minReplicas` (the prod
overlay patches both).

**http-proxy** keeps ratelimit state in memory and must stay at one replica.

## Rolling restarts

A rolling update of the gateway replaces one pod at a time (highest ordinal first,
PodDisruptionBudget `maxUnavailable: 1`). On SIGTERM a pod closes each shard with a
resumable close frame and saves the session ids and sequence numbers to Redis, which
takes about 25 s, hence `terminationGracePeriodSeconds: 45`. The replacement pod keeps
the same name, so it gets the same ordinal and the same shards, loads those sessions and
sends RESUME instead of IDENTIFY. Discord keeps a closed session resumable for a short
while: the sessions are stored for 240 s, so a pod must be back within about that time
or it falls back to a full identify (which counts against the daily session start
limit). Wait for each pod to become Ready (`/readyz`) before touching the next one;
the StatefulSet controller does this for you.
