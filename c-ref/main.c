#include <stdio.h>
#include <stdlib.h>
#include <X11/Xlib.h>
#include <X11/extensions/Xrandr.h>

#define PR(fmt, ...) printf("DEBUG: " fmt "\n", ##__VA_ARGS__)
#define EXIT(fmt, ...)                                             \
	do {                                                       \
		fprintf(stderr, "Oops: " fmt "\n", ##__VA_ARGS__); \
		exit(1);                                           \
	} while (0)

int main(void)
{
	Display *display;

	display = XOpenDisplay(0);

	if (!display)
		EXIT("Failed to open display 0");

	PR("%p", (void *)display);

	return 0;
}
