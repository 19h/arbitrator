FROM psychonaut/rust-nightly:2019-05-07

ADD . /my-source

RUN    cd /my-source \
    && cargo rustc --verbose --release -- -C target-cpu=native \
    && mv /my-source/target/release/int19h /int19h \
    && rm -rfv /my-source

CMD ["/int19h"]
