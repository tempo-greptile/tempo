include "kubernetes" {
  path = find_in_parent_folders("kubernetes.hcl")
}

include "cluster" {
  path = find_in_parent_folders("cluster.hcl")
  expose = true
}

include "root" {
  path = find_in_parent_folders("root.hcl")
}

terraform {
  source = find_in_parent_folders("modules/kubernetes/bootstrap")
}


inputs = {
  # Cluster configuration
  cluster_name = "dev-mgmt-dal-01"
  
  # Tailscale operator configuration
  tailscale_operator_version = "1.86.2"
  
  # ArgoCD configuration
  argocd_version            = "7.6.12"
  argocd_tailscale_hostname = "argocd-dev"

  coredns_version = "1.43.3"

  onepassword_secret_token = include.cluster.locals.onepassword_token
  onepassword_credentials = include.cluster.locals.onepassword_connect_credentials
  onepassword_connect_version = "2.0.3"

  external_clusters = {
    "dev-mgmt-dal-01" = "dev-mgmt-dal-01-ts-operator.tail388b2e.ts.net"
  }
}