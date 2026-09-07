/*
 * wawona_wpm.h — C ABI for the Wawona Runtime package manager.
 * Declared weak by wawona-dispatch; Apple mobile links libwpm.a in-process.
 */
#ifndef WAWONA_WPM_H
#define WAWONA_WPM_H

#ifdef __cplusplus
extern "C" {
#endif

/** Same shape as phoon_main / a normal C main. */
int wpm_main(int argc, char **argv);

#ifdef __cplusplus
}
#endif

#endif /* WAWONA_WPM_H */
