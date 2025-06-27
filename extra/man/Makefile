# Man page generation
.PHONY: all clean install

SCDOC = scdoc
MANDIR = /usr/local/share/man

all: rio.1 rio.5 rio-bindings.5

rio.1: rio.1.scd
	$(SCDOC) < $< > $@

rio.5: rio.5.scd
	$(SCDOC) < $< > $@

rio-bindings.5: rio-bindings.5.scd
	$(SCDOC) < $< > $@

install: all
	install -Dm644 rio.1 $(DESTDIR)$(MANDIR)/man1/rio.1
	install -Dm644 rio.5 $(DESTDIR)$(MANDIR)/man5/rio.5
	install -Dm644 rio-bindings.5 $(DESTDIR)$(MANDIR)/man5/rio-bindings.5

clean:
	rm -f rio.1 rio.5 rio-bindings.5