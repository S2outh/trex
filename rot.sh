while [ true ]; do
  trex-cli set --az=1 --el=-0.3
  sleep 3
  trex-cli set --az=1 --el=-0.3
  sleep 3
  trex-cli set --az=-1 --el=-0.3
  sleep 3
  trex-cli set --az=-1 --el=0.3
  sleep 3
done
