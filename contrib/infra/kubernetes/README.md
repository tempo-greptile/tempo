# Kubernetes cluster setup

## Bootstrapping

1. Manually create an Omni join token in the Omni UI (select the `Generic Image (amd64)` option). When you get the URL, copy its' contents and add `ip=dhcp` to the kernel cmdline arguments. You should end up with something like this:

```
#!ipxe

imgfree
kernel https://pxe.factory.talos.dev/image/<redacted>/v1.10.1/kernel-amd64 talos.platform=metal console=tty0 init_on_alloc=1 slab_nomerge pti=on consoleblank=0 nvme_core.io_timeout=4294967295 printk.devkmsg=on ima_template=ima-ng ima_appraise=fix ima_hash=sha512 selinux=1 siderolink.api=https://tempoxyz.siderolink.na-west-1.omni.siderolabs.io?jointoken=<redacted> talos.events.sink=[fdae:41e4:649b:9303::1]:8090 talos.logging.kernel=tcp://[fdae:41e4:649b:9303::1]:8092 ip=dhcp
initrd https://pxe.factory.talos.dev/image/<redacted>/v1.10.1/initramfs-amd64.xz
boot
```

2. Boot the instances using the iPXE script

```
#!ipxe

imgfree
kernel https://pxe.factory.talos.dev/image/4e896e215e6edf58076c797dd14e57f72c51129a5fe56b361d68c2a77a3ce3db/v1.10.5/kernel-amd64 talos.platform=metal console=tty0 init_on_alloc=1 slab_nomerge pti=on consoleblank=0 nvme_core.io_timeout=4294967295 printk.devkmsg=on ima_template=ima-ng ima_appraise=fix ima_hash=sha512 selinux=1 siderolink.api=https://tempoxyz.siderolink.na-west-1.omni.siderolabs.io?jointoken=k69YUs3w53umAHsmBqbSv8Ipw1J2AfDFoUDPB79K9VY talos.events.sink=[fdae:41e4:649b:9303::1]:8090 talos.logging.kernel=tcp://[fdae:41e4:649b:9303::1]:8092 ip=dhcp
initrd https://pxe.factory.talos.dev/image/4e896e215e6edf58076c797dd14e57f72c51129a5fe56b361d68c2a77a3ce3db/v1.10.5/initramfs-amd64.xz
boot
```

3. Create the cluster in Omni with the following config patches:

```
cluster:
  coreDNS:
    disabled: true

machine:
  kubelet:
    clusterDNS:
    - 10.96.0.10
```