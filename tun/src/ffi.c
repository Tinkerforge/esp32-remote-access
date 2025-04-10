#include <linux/if.h>
#include <linux/if_tun.h>
#include <linux/string.h>
#include <stdio.h>
#include <sys/ioctl.h>
#include <errno.h>
#include <fcntl.h>
#include <unistd.h>
#include <arpa/inet.h>
#include <net/route.h>

int strscpy_pad(char *dst, const char * src, int size)
{
   int written = size;
   while (*dst != 0 && size != 0)
   {
      *dst = *src;
      src++;
      dst++;
      size--;
   }
   written -= size;
   while (size != 0)
   {
      dst = 0;
      dst++;
      size--;
   }
   dst = 0;
   return written;
}

int tun_alloc(char *dev, const char *self_ip, const char *peer_ip)
{
    struct ifreq ifr;
    struct sockaddr_in sai;
    struct rtentry rt;
    int fd, err, sock;

    if( (fd = open("/dev/net/tun", O_RDWR)) < 0 )
    {
      return -1;
    }

    memset(&ifr, 0, sizeof(ifr));

    /* Flags: IFF_TUN   - TUN device (no Ethernet headers)
     *        IFF_TAP   - TAP device
     *
     *        IFF_NO_PI - Do not provide packet information
     */
    ifr.ifr_flags = IFF_TUN;
    if( *dev )
       strscpy_pad(ifr.ifr_name, dev, IFNAMSIZ);

    if( (err = ioctl(fd, TUNSETIFF, (void *) &ifr)) < 0 )
    {
       printf("ioctl failed: %s\n", strerror(errno));
       close(fd);
       return err;
    }
    strcpy(dev, ifr.ifr_name);

    sock = socket(AF_INET, SOCK_DGRAM, 0);
    if (sock < 0)
    {
      printf("opening socket failed: %s\n", strerror(errno));
      close(fd);
      return sock;
    }

    memset(&sai, 0, sizeof(struct sockaddr));
    sai.sin_family = AF_INET;
    sai.sin_port = 0;
    sai.sin_addr.s_addr = inet_addr(self_ip);

    memcpy(&ifr.ifr_addr, &sai, sizeof(struct sockaddr));
    if ( (err = ioctl(sock, SIOCSIFADDR, &ifr)) < 0)
    {
      printf("setting ip address failed: %s\n", strerror(errno));
      close(fd);
      close(sock);
      return err;
    }

    sai.sin_addr.s_addr = inet_addr(peer_ip);
    memcpy(&ifr.ifr_dstaddr, &sai, sizeof(struct sockaddr));
    if ( (err = ioctl(sock, SIOCSIFDSTADDR, &ifr)) < 0)
    {
      printf("setting peer address failed: %s\n", strerror(errno));
      close(fd);
      close(sock);
      return err;
    }

    if ( (err = ioctl(sock, SIOCGIFFLAGS, &ifr)) < 0)
    {
      printf("getting flags failed: %s\n", strerror(errno));
      close(fd);
      close(sock);
      return err;
    }
    ifr.ifr_flags |= IFF_UP | IFF_RUNNING;

    if ( (err = ioctl(sock, SIOCSIFFLAGS, &ifr)) < 0) {
      printf("setting flags failed: %s\n", strerror(errno));
      close(fd);
      close(sock);
      return err;
    }

    memset(&rt, 0, sizeof(struct rtentry));
    struct sockaddr_in *addr = (struct sockaddr_in *)&rt.rt_dst;
    addr->sin_family = AF_INET;
    addr->sin_addr.s_addr = inet_addr(peer_ip);

    addr = (struct sockaddr_in *)&rt.rt_genmask;
    addr->sin_family = AF_INET;
    addr->sin_addr.s_addr = inet_addr("255.255.255.255");
    rt.rt_flags = RTF_UP | RTF_STATIC;
    rt.rt_metric = 0;
    rt.rt_dev = dev;

    if( (err = ioctl(sock, SIOCADDRT, &rt)) < 0) {
      printf("adding route failed: %s\n", strerror(errno));
      close(sock);
      close(fd);
      return err;
    }

    close(sock);

    return fd;
}
